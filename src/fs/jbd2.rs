//! JBD2 (Journal Block Device) journaling layer
//!
//! Implements an in-memory journal state machine modelling the core of Linux's
//! JBD2 layer: transactions progress through `Running -> Locked -> Flush ->
//! Commit -> Finished`, buffers are attached to the running transaction, and a
//! checkpoint list tracks blocks that have been safely written back to the
//! target filesystem and can be reclaimed.
//!
//! The type also implements [`FileSystem`] so a mountable view of the journal
//! state (`/transactions`, `/buffers`, `/checkpoint`, `/state`) is available for
//! introspection, mirroring the debug exposure of the real layer.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

// ---------------------------------------------------------------------------
// Inode numbering (deterministic, no allocation on the read path)
// ---------------------------------------------------------------------------

const ROOT_INODE: InodeNumber = 1;
const TRANSACTIONS_INODE: InodeNumber = 2;
const BUFFERS_INODE: InodeNumber = 3;
const CHECKPOINT_INODE: InodeNumber = 4;
const STATE_INODE: InodeNumber = 5;
/// Per-block file inodes start here; block N maps to `BLOCK_BASE + N`.
const BLOCK_BASE: InodeNumber = 1000;

/// Kind of journal entry, recoverable from an inode number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Jbd2Kind {
    Root,
    Transactions,
    Buffers,
    Checkpoint,
    State,
    Block(u64),
}

impl Jbd2Kind {
    fn to_inode(self) -> InodeNumber {
        match self {
            Jbd2Kind::Root => ROOT_INODE,
            Jbd2Kind::Transactions => TRANSACTIONS_INODE,
            Jbd2Kind::Buffers => BUFFERS_INODE,
            Jbd2Kind::Checkpoint => CHECKPOINT_INODE,
            Jbd2Kind::State => STATE_INODE,
            Jbd2Kind::Block(n) => BLOCK_BASE + n,
        }
    }

    fn from_inode(inode: InodeNumber) -> Option<Jbd2Kind> {
        match inode {
            ROOT_INODE => Some(Jbd2Kind::Root),
            TRANSACTIONS_INODE => Some(Jbd2Kind::Transactions),
            BUFFERS_INODE => Some(Jbd2Kind::Buffers),
            CHECKPOINT_INODE => Some(Jbd2Kind::Checkpoint),
            STATE_INODE => Some(Jbd2Kind::State),
            n if n >= BLOCK_BASE => Some(Jbd2Kind::Block(n - BLOCK_BASE)),
            _ => None,
        }
    }

    fn file_type(self) -> FileType {
        match self {
            Jbd2Kind::Root => FileType::Directory,
            _ => FileType::Regular,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Jbd2Kind::Root => "/",
            Jbd2Kind::Transactions => "transactions",
            Jbd2Kind::Buffers => "buffers",
            Jbd2Kind::Checkpoint => "checkpoint",
            Jbd2Kind::State => "state",
            Jbd2Kind::Block(_) => "block",
        }
    }
}

// ---------------------------------------------------------------------------
// Journal state machine
// ---------------------------------------------------------------------------

/// Lifecycle of a JBD2 transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// Accepting new buffers (`T_RUNNING`).
    Running,
    /// Closed to new buffers, waiting for the lock (`T_LOCKED`).
    Locked,
    /// Flushing data buffers to disk (`T_FLUSH`).
    Flush,
    /// Committing the descriptor block (`T_COMMIT`).
    Commit,
    /// Commit record written, waiting for checkpoint (`T_FINISHED`).
    Finished,
}

impl TransactionState {
    fn as_str(self) -> &'static str {
        match self {
            TransactionState::Running => "RUNNING",
            TransactionState::Locked => "LOCKED",
            TransactionState::Flush => "FLUSH",
            TransactionState::Commit => "COMMIT",
            TransactionState::Finished => "FINISHED",
        }
    }
}

/// A single journaled buffer (block).
#[derive(Debug, Clone)]
pub struct JournalBuffer {
    /// Block number on the backing device.
    pub block_nr: u64,
    /// Sequence number of the transaction that owns this buffer.
    pub tid: u64,
    /// True if the buffer has been modified since it was loaded.
    pub dirty: bool,
    /// Payload bytes.
    pub data: Vec<u8>,
}

/// A JBD2 transaction grouping a set of buffer modifications.
#[derive(Debug, Clone)]
pub struct Transaction {
    /// Transaction sequence ID.
    pub tid: u64,
    /// Current state in the lifecycle.
    pub state: TransactionState,
    /// Buffers attached to this transaction.
    pub buffers: Vec<JournalBuffer>,
    /// Timestamp (ms) when the transaction was created.
    pub created: u64,
}

/// In-memory JBD2 journal state.
#[derive(Debug)]
pub struct JournalState {
    /// All transactions keyed by TID.
    pub transactions: BTreeMap<u64, Transaction>,
    /// Buffers that have been checkpointed (safe to reclaim).
    pub checkpoint_list: Vec<JournalBuffer>,
    /// Next transaction ID to allocate.
    next_tid: u64,
    /// Sequence number of the most recently completed transaction.
    pub last_commit_tid: u64,
}

impl JournalState {
    fn new() -> Self {
        Self {
            transactions: BTreeMap::new(),
            checkpoint_list: Vec::new(),
            next_tid: 1,
            last_commit_tid: 0,
        }
    }

    /// Allocate the next transaction ID.
    fn next_transaction_id(&mut self) -> u64 {
        let tid = self.next_tid;
        self.next_tid += 1;
        tid
    }

    /// Begin a new running transaction.
    pub fn start_transaction(&mut self, now: u64) -> u64 {
        let tid = self.next_transaction_id();
        self.transactions.insert(
            tid,
            Transaction {
                tid,
                state: TransactionState::Running,
                buffers: Vec::new(),
                created: now,
            },
        );
        tid
    }

    /// Attach a buffer to the running transaction with the given TID.
    pub fn add_buffer(&mut self, tid: u64, block_nr: u64, data: Vec<u8>) -> FsResult<()> {
        let txn = self.transactions.get_mut(&tid).ok_or(FsError::NotFound)?;
        if txn.state != TransactionState::Running {
            return Err(FsError::NotSupported);
        }
        txn.buffers.push(JournalBuffer {
            block_nr,
            tid,
            dirty: true,
            data,
        });
        Ok(())
    }

    /// Advance a transaction to the next state in its lifecycle.
    pub fn advance_state(&mut self, tid: u64) -> FsResult<TransactionState> {
        let txn = self.transactions.get_mut(&tid).ok_or(FsError::NotFound)?;
        txn.state = match txn.state {
            TransactionState::Running => TransactionState::Locked,
            TransactionState::Locked => TransactionState::Flush,
            TransactionState::Flush => TransactionState::Commit,
            TransactionState::Commit => TransactionState::Finished,
            TransactionState::Finished => return Ok(TransactionState::Finished),
        };
        Ok(txn.state)
    }

    /// Move a finished transaction's buffers to the checkpoint list and
    /// record the commit. Returns the number of buffers checkpointed.
    pub fn checkpoint(&mut self, tid: u64) -> FsResult<usize> {
        let txn = self.transactions.get_mut(&tid).ok_or(FsError::NotFound)?;
        if txn.state != TransactionState::Finished {
            return Err(FsError::NotSupported);
        }
        let n = txn.buffers.len();
        let mut buffers = core::mem::take(&mut txn.buffers);
        for b in &mut buffers {
            b.dirty = false;
        }
        self.checkpoint_list.extend(buffers);
        self.last_commit_tid = tid;
        // Finished transactions are retained for inspection but emptied.
        Ok(n)
    }

    /// Drop checkpointed buffers whose data has been written back.
    pub fn drop_checkpointed(&mut self, block_nr: u64) -> FsResult<()> {
        self.checkpoint_list.retain(|b| b.block_nr != block_nr);
        Ok(())
    }

    /// Look up a buffer by block number across all transactions + checkpoint.
    fn find_buffer(&self, block_nr: u64) -> Option<JournalBuffer> {
        for txn in self.transactions.values() {
            if let Some(b) = txn.buffers.iter().find(|b| b.block_nr == block_nr) {
                return Some(b.clone());
            }
        }
        self.checkpoint_list
            .iter()
            .find(|b| b.block_nr == block_nr)
            .cloned()
    }
}

// ---------------------------------------------------------------------------
// Filesystem wrapper
// ---------------------------------------------------------------------------

/// JBD2 journaling filesystem.
#[derive(Debug)]
pub struct Jbd2FileSystem {
    /// In-memory journal state machine.
    journal: RwLock<JournalState>,
    /// Block size in bytes (defaults to 4096).
    block_size: u32,
}

impl Jbd2FileSystem {
    /// Create a new JBD2 journal instance.
    pub fn new() -> FsResult<Self> {
        Ok(Self {
            journal: RwLock::new(JournalState::new()),
            block_size: 4096,
        })
    }

    /// Begin a new running transaction and return its TID.
    pub fn start_transaction(&self, now: u64) -> u64 {
        self.journal.write().start_transaction(now)
    }

    /// Attach a buffer to a running transaction.
    pub fn add_buffer(&self, tid: u64, block_nr: u64, data: Vec<u8>) -> FsResult<()> {
        self.journal.write().add_buffer(tid, block_nr, data)
    }

    /// Advance a transaction to its next lifecycle state.
    pub fn advance_state(&self, tid: u64) -> FsResult<TransactionState> {
        self.journal.write().advance_state(tid)
    }

    /// Checkpoint a finished transaction.
    pub fn checkpoint(&self, tid: u64) -> FsResult<usize> {
        self.journal.write().checkpoint(tid)
    }

    /// Generate the textual content for a state-inspection inode.
    fn generate_content(&self, kind: Jbd2Kind) -> FsResult<String> {
        let journal = self.journal.read();
        Ok(match kind {
            Jbd2Kind::Root => return Err(FsError::IsADirectory),
            Jbd2Kind::Transactions => {
                let mut out = String::new();
                for txn in journal.transactions.values() {
                    out.push_str(&format!(
                        "tid={tid} state={state} buffers={n} created={created}\n",
                        tid = txn.tid,
                        state = txn.state.as_str(),
                        n = txn.buffers.len(),
                        created = txn.created,
                    ));
                }
                out
            }
            Jbd2Kind::Buffers => {
                let mut out = String::new();
                for txn in journal.transactions.values() {
                    for b in &txn.buffers {
                        out.push_str(&format!(
                            "block={block_nr} tid={tid} dirty={dirty} bytes={len}\n",
                            block_nr = b.block_nr,
                            tid = b.tid,
                            dirty = b.dirty,
                            len = b.data.len(),
                        ));
                    }
                }
                out
            }
            Jbd2Kind::Checkpoint => {
                let mut out = String::new();
                for b in &journal.checkpoint_list {
                    out.push_str(&format!(
                        "block={block_nr} tid={tid} dirty={dirty} bytes={len}\n",
                        block_nr = b.block_nr,
                        tid = b.tid,
                        dirty = b.dirty,
                        len = b.data.len(),
                    ));
                }
                out
            }
            Jbd2Kind::State => {
                let running = journal
                    .transactions
                    .values()
                    .filter(|t| t.state == TransactionState::Running)
                    .count();
                let finished = journal
                    .transactions
                    .values()
                    .filter(|t| t.state == TransactionState::Finished)
                    .count();
                format!(
                    "block_size={bs}\n\
                     next_tid={next}\n\
                     last_commit_tid={last}\n\
                     active_transactions={active}\n\
                     running_transactions={running}\n\
                     finished_transactions={finished}\n\
                     checkpoint_buffers={cp}\n",
                    bs = self.block_size,
                    next = journal.next_tid,
                    last = journal.last_commit_tid,
                    active = journal.transactions.len(),
                    running = running,
                    finished = finished,
                    cp = journal.checkpoint_list.len(),
                )
            }
            Jbd2Kind::Block(block_nr) => {
                let buf = journal.find_buffer(block_nr).ok_or(FsError::NotFound)?;
                format!(
                    "block_nr={block_nr}\ntid={tid}\ndirty={dirty}\nbytes={len}\n",
                    block_nr = buf.block_nr,
                    tid = buf.tid,
                    dirty = buf.dirty,
                    len = buf.data.len(),
                )
            }
        })
    }

    fn metadata_for(&self, kind: Jbd2Kind) -> FileMetadata {
        let file_type = kind.file_type();
        let size = if file_type == FileType::Directory {
            0
        } else {
            self.generate_content(kind)
                .map(|c| c.len() as u64)
                .unwrap_or(0)
        };
        FileMetadata {
            inode: kind.to_inode(),
            file_type,
            size,
            permissions: match file_type {
                FileType::Directory => FilePermissions::default_directory(),
                _ => FilePermissions::from_octal(0o444),
            },
            uid: 0,
            gid: 0,
            created: 0,
            modified: 0,
            accessed: 0,
            link_count: 1,
            device_id: None,
        }
    }
}

impl FileSystem for Jbd2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Jbd2
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let journal = self.journal.read();
        let block_size = self.block_size;
        let total_blocks = 1024u64;
        let used: u64 = journal
            .transactions
            .values()
            .flat_map(|t| t.buffers.iter())
            .map(|b| (b.data.len() as u64 + block_size as u64 - 1) / block_size as u64)
            .sum::<u64>()
            + journal
                .checkpoint_list
                .iter()
                .map(|b| (b.data.len() as u64 + block_size as u64 - 1) / block_size as u64)
                .sum::<u64>();
        Ok(FileSystemStats {
            total_blocks,
            free_blocks: total_blocks.saturating_sub(used),
            available_blocks: total_blocks.saturating_sub(used),
            total_inodes: 4096,
            free_inodes: 4096,
            block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // Journal files are created through the transaction API, not via VFS.
        Err(FsError::NotSupported)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let kind = resolve_path(path)?;
        // Verify block files reference an existing buffer.
        if let Jbd2Kind::Block(n) = kind {
            if self.journal.read().find_buffer(n).is_none() {
                return Err(FsError::NotFound);
            }
        }
        Ok(kind.to_inode())
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let kind = Jbd2Kind::from_inode(inode).ok_or(FsError::NotFound)?;
        if kind.file_type() == FileType::Directory {
            return Err(FsError::IsADirectory);
        }
        let content = self.generate_content(kind)?.into_bytes();
        let len = content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(start + buffer.len(), content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&content[start..end]);
        Ok(n)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        // Writes go through the transaction API (`add_buffer`); raw inode
        // writes are not part of the journal interface.
        Err(FsError::NotSupported)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let kind = Jbd2Kind::from_inode(inode).ok_or(FsError::NotFound)?;
        Ok(self.metadata_for(kind))
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let kind = Jbd2Kind::from_inode(inode).ok_or(FsError::NotFound)?;
        if kind != Jbd2Kind::Root {
            return Err(FsError::NotADirectory);
        }
        let journal = self.journal.read();
        let mut entries = Vec::new();
        for k in [
            Jbd2Kind::Transactions,
            Jbd2Kind::Buffers,
            Jbd2Kind::Checkpoint,
            Jbd2Kind::State,
        ] {
            entries.push(DirectoryEntry {
                name: k.name().to_string(),
                inode: k.to_inode(),
                file_type: FileType::Regular,
            });
        }
        // Expose one file per known block number.
        let mut block_nrs: BTreeMap<u64, ()> = BTreeMap::new();
        for txn in journal.transactions.values() {
            for b in &txn.buffers {
                block_nrs.insert(b.block_nr, ());
            }
        }
        for b in &journal.checkpoint_list {
            block_nrs.insert(b.block_nr, ());
        }
        for &block_nr in block_nrs.keys() {
            entries.push(DirectoryEntry {
                name: format!("block-{}", block_nr),
                inode: Jbd2Kind::Block(block_nr).to_inode(),
                file_type: FileType::Regular,
            });
        }
        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        // Flush all finished transactions through the checkpoint pipeline.
        let finished_tids: Vec<u64> = {
            let journal = self.journal.read();
            journal
                .transactions
                .values()
                .filter(|t| t.state == TransactionState::Finished)
                .map(|t| t.tid)
                .collect()
        };
        for tid in finished_tids {
            let _ = self.journal.write().checkpoint(tid);
        }
        Ok(())
    }
}

/// Resolve a path relative to the journal root.
fn resolve_path(path: &str) -> FsResult<Jbd2Kind> {
    let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
    if components.is_empty() {
        return Ok(Jbd2Kind::Root);
    }
    match components[0] {
        "transactions" if components.len() == 1 => Ok(Jbd2Kind::Transactions),
        "buffers" if components.len() == 1 => Ok(Jbd2Kind::Buffers),
        "checkpoint" if components.len() == 1 => Ok(Jbd2Kind::Checkpoint),
        "state" if components.len() == 1 => Ok(Jbd2Kind::State),
        name if name.starts_with("block-") && components.len() == 1 => {
            let n = name["block-".len()..]
                .parse::<u64>()
                .map_err(|_| FsError::NotFound)?;
            Ok(Jbd2Kind::Block(n))
        }
        _ => Err(FsError::NotFound),
    }
}
