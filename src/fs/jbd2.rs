//! JBD2 (Journal Block Device) journaling layer implementation
//!
//! This module implements a journaling block device layer that provides
//! crash-safe write semantics via a circular log. It is used by ext4 and
//! other filesystems for data integrity.
//!
//! Design:
//! - `BlockDevice` trait abstracts the underlying storage.
//! - On-disk structures: `JournalSuperblock` (magic 0xC03B3998),
//!   `JournalHeader`, `JournalBlockTag`, `JournalCommitHeader`.
//! - In-memory: `TransactionState` (Running/Locked/Committing/Finished),
//!   `JournalBlock`, `TransactionHandle`, `Journal` struct.
//! - Transaction lifecycle: start -> get_write_access/get_create_access ->
//!   dirty_metadata -> commit -> checkpoint.
//! - Recovery: replay journal after crash via `journal_recover`.

use super::{
    DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats, FileSystemType,
    FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    string::String,
    sync::Arc,
    vec,
    vec::Vec,
};
use spin::Mutex;

/// JBD2 superblock magic number
const JBD2_MAGIC_NUMBER: u32 = 0xC03B3998;

/// JBD2 superblock block type
const JBD2_SUPERBLOCK_V1: u32 = 0;
const JBD2_SUPERBLOCK_V2: u32 = 1;

/// JBD2 descriptor block type
const JBD2_DESCRIPTOR_BLOCK: u32 = 2;

/// JBD2 commit block type
const JBD2_COMMIT_BLOCK: u32 = 3;

/// JBD2 revoked block tag flag
const JBD2_FLAG_ESCAPE: u32 = 1;
const JBD2_FLAG_SAME_UUID: u32 = 2;
const JBD2_FLAG_DELETED: u32 = 4;
const JBD2_FLAG_LAST_TAG: u32 = 8;

/// Block device abstraction for the journal.
pub trait BlockDevice: Send + Sync {
    /// Read a block at `block_num` into `buffer`.
    fn read_block(&self, block_num: u64, buffer: &mut [u8]) -> FsResult<()>;

    /// Write `buffer` to the block at `block_num`.
    fn write_block(&self, block_num: u64, buffer: &[u8]) -> FsResult<()>;

    /// Block size in bytes.
    fn block_size(&self) -> u32;

    /// Total number of blocks on the device.
    fn num_blocks(&self) -> u64;
}

// ============================================================================
// On-disk structures
// ============================================================================

/// JBD2 journal superblock (lives at the first block of the journal area).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct JournalSuperblock {
    pub s_header: JournalHeader,
    pub s_blocksize: u32,
    pub s_maxlen: u32,
    pub s_first: u32,
    pub s_sequence: u32,
    pub s_start: u32,
    pub s_errno: u32,
    pub s_feature_compat: u32,
    pub s_feature_incompat: u32,
    pub s_feature_ro_compat: u32,
    pub s_uuid: [u8; 16],
    pub s_nr_users: u32,
    pub s_dynsuper: u32,
    pub s_max_transaction: u32,
    pub s_max_trans_data: u32,
    pub s_padding: [u32; 40],
    pub s_checksum: u32,
}

/// Common journal block header.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct JournalHeader {
    pub h_magic: u32,
    pub h_blocktype: u32,
    pub h_sequence: u32,
}

/// JBD2 block tag (in descriptor blocks, describes a logged block).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct JournalBlockTag {
    pub t_blocknr: u32,
    pub t_flags: u32,
}

/// JBD2 commit block header.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct JournalCommitHeader {
    pub h_magic: u32,
    pub h_blocktype: u32,
    pub h_sequence: u32,
    pub h_commit_sec: u32,
    pub h_commit_nsec: u32,
}

// ============================================================================
// In-memory structures
// ============================================================================

/// State of a journal transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionState {
    /// Transaction is accepting new block registrations.
    Running,
    /// Transaction is locked (no new blocks).
    Locked,
    /// Transaction is being written to the journal log.
    Committing,
    /// Transaction has been committed to the log.
    Finished,
}

/// A block registered in the current transaction.
#[derive(Debug, Clone)]
pub struct JournalBlock {
    /// The filesystem block number this entry protects.
    pub fs_block: u64,
    /// The block's data content.
    pub data: Vec<u8>,
    /// Whether this is a new (created) block vs. an existing one.
    pub is_new: bool,
    /// Whether the block has been marked dirty.
    pub dirty: bool,
}

/// Handle to an active transaction, returned by `journal_start`.
#[derive(Debug)]
pub struct TransactionHandle {
    /// Transaction sequence number.
    pub sequence: u32,
    /// Blocks registered in this transaction.
    pub blocks: Vec<JournalBlock>,
    /// Current state.
    pub state: TransactionState,
}

impl TransactionHandle {
    fn new(sequence: u32) -> Self {
        Self {
            sequence,
            blocks: Vec::new(),
            state: TransactionState::Running,
        }
    }
}

/// The journal itself.
pub struct Journal {
    /// Underlying block device.
    device: Arc<dyn BlockDevice>,
    /// Starting block of the journal area on the device.
    start: u64,
    /// Number of blocks in the journal log.
    count: u32,
    /// Block size.
    block_size: u32,
    /// Cached superblock.
    superblock: Mutex<JournalSuperblock>,
    /// Current running transaction handle (if any).
    current_handle: Mutex<Option<TransactionHandle>>,
    /// Next sequence number to assign.
    next_sequence: Mutex<u32>,
    /// Committed-but-not-yet-checkpointed transactions.
    /// Keyed by sequence number, value is the list of (fs_block, log_block) pairs.
    checkpoint_list: Mutex<BTreeMap<u32, Vec<(u64, u32)>>>,
    /// Committed transactions log mapping: sequence -> log blocks used.
    committed_log: Mutex<BTreeMap<u32, Vec<u32>>>,
}

impl core::fmt::Debug for Journal {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Journal")
            .field("start", &self.start)
            .field("count", &self.count)
            .field("block_size", &self.block_size)
            .field("next_sequence", &*self.next_sequence.lock())
            .finish()
    }
}

impl Journal {
    /// Create a new journal, reading and validating the on-disk superblock.
    pub fn new(device: Arc<dyn BlockDevice>, start: u64, count: u32) -> FsResult<Self> {
        let block_size = device.block_size();
        if block_size == 0 {
            return Err(FsError::InvalidArgument);
        }

        let mut journal = Self {
            device,
            start,
            count,
            block_size,
            superblock: Mutex::new(unsafe { core::mem::zeroed() }),
            current_handle: Mutex::new(None),
            next_sequence: Mutex::new(1),
            checkpoint_list: Mutex::new(BTreeMap::new()),
            committed_log: Mutex::new(BTreeMap::new()),
        };

        journal.read_and_validate_superblock()?;
        Ok(journal)
    }

    fn read_and_validate_superblock(&mut self) -> FsResult<()> {
        let mut buf = vec![0u8; self.block_size as usize];
        self.device
            .read_block(self.start, &mut buf)
            .map_err(|_| FsError::IoError)?;
        let sb = unsafe {
            core::ptr::read_unaligned(buf.as_ptr() as *const JournalSuperblock)
        };
        if sb.s_header.h_magic != JBD2_MAGIC_NUMBER {
            return Err(FsError::InvalidArgument);
        }
        if sb.s_blocksize != self.block_size {
            return Err(FsError::InvalidArgument);
        }
        *self.superblock.lock() = sb;
        *self.next_sequence.lock() = sb.s_sequence.max(1);
        Ok(())
    }

    fn write_superblock(&self) -> FsResult<()> {
        let sb = *self.superblock.lock();
        let mut buf = vec![0u8; self.block_size as usize];
        let sb_bytes = unsafe {
            core::slice::from_raw_parts(
                &sb as *const JournalSuperblock as *const u8,
                core::mem::size_of::<JournalSuperblock>(),
            )
        };
        let copy_len = core::cmp::min(sb_bytes.len(), buf.len());
        buf[..copy_len].copy_from_slice(&sb_bytes[..copy_len]);
        self.device
            .write_block(self.start, &buf)
            .map_err(|_| FsError::IoError)
    }

    /// Read a block from the journal log area (offset by `start`).
    fn read_log_block(&self, log_offset: u32) -> FsResult<Vec<u8>> {
        let mut buf = vec![0u8; self.block_size as usize];
        let block = self.start + log_offset as u64;
        self.device
            .read_block(block, &mut buf)
            .map_err(|_| FsError::IoError)?;
        Ok(buf)
    }

    /// Write a block to the journal log area.
    fn write_log_block(&self, log_offset: u32, data: &[u8]) -> FsResult<()> {
        if data.len() != self.block_size as usize {
            return Err(FsError::InvalidArgument);
        }
        let block = self.start + log_offset as u64;
        self.device
            .write_block(block, data)
            .map_err(|_| FsError::IoError)
    }

    // ------------------------------------------------------------------
    // Transaction lifecycle
    // ------------------------------------------------------------------

    /// Start a new transaction. Returns a handle.
    pub fn journal_start(&self) -> FsResult<()> {
        let mut handle = self.current_handle.lock();
        if handle.is_some() {
            // A transaction is already running; nest by returning Ok.
            return Ok(());
        }
        let seq = {
            let mut ns = self.next_sequence.lock();
            let v = *ns;
            *ns += 1;
            v
        };
        *handle = Some(TransactionHandle::new(seq));
        Ok(())
    }

    /// Stop the current transaction (does not commit — use commit for that).
    pub fn journal_stop(&self) -> FsResult<()> {
        let mut handle = self.current_handle.lock();
        if let Some(h) = handle.as_mut() {
            h.state = TransactionState::Locked;
        }
        Ok(())
    }

    /// Extend the current transaction's buffer credits (no-op in this impl
    /// since we don't use credit accounting, but we validate the handle).
    pub fn journal_extend(&self, _blocks: u32) -> FsResult<()> {
        let handle = self.current_handle.lock();
        if handle.is_none() {
            return Err(FsError::InvalidArgument);
        }
        Ok(())
    }

    /// Register an existing block for write access in the current transaction.
    /// Reads the block's current content from the device.
    pub fn journal_get_write_access(&self, fs_block: u64) -> FsResult<()> {
        let mut handle = self.current_handle.lock();
        let h = handle.as_mut().ok_or(FsError::InvalidArgument)?;
        if h.state != TransactionState::Running {
            return Err(FsError::InvalidArgument);
        }
        // Check if already registered.
        if h.blocks.iter().any(|b| b.fs_block == fs_block) {
            return Ok(());
        }
        let mut data = vec![0u8; self.block_size as usize];
        self.device
            .read_block(fs_block, &mut data)
            .map_err(|_| FsError::IoError)?;
        h.blocks.push(JournalBlock {
            fs_block,
            data,
            is_new: false,
            dirty: false,
        });
        Ok(())
    }

    /// Register a newly created block for write access (no initial read needed).
    pub fn journal_get_create_access(&self, fs_block: u64) -> FsResult<()> {
        let mut handle = self.current_handle.lock();
        let h = handle.as_mut().ok_or(FsError::InvalidArgument)?;
        if h.state != TransactionState::Running {
            return Err(FsError::InvalidArgument);
        }
        if h.blocks.iter().any(|b| b.fs_block == fs_block) {
            return Ok(());
        }
        h.blocks.push(JournalBlock {
            fs_block,
            data: vec![0u8; self.block_size as usize],
            is_new: true,
            dirty: false,
        });
        Ok(())
    }

    /// Mark a registered block as dirty (its content has been modified and
    /// should be journaled).
    pub fn journal_dirty_metadata(&self, fs_block: u64, data: &[u8]) -> FsResult<()> {
        let mut handle = self.current_handle.lock();
        let h = handle.as_mut().ok_or(FsError::InvalidArgument)?;
        if h.state != TransactionState::Running {
            return Err(FsError::InvalidArgument);
        }
        let block_size = self.block_size as usize;
        for b in &mut h.blocks {
            if b.fs_block == fs_block {
                if data.len() != block_size {
                    return Err(FsError::InvalidArgument);
                }
                b.data.copy_from_slice(data);
                b.dirty = true;
                return Ok(());
            }
        }
        Err(FsError::NotFound)
    }

    // ------------------------------------------------------------------
    // Commit & checkpoint
    // ------------------------------------------------------------------

    /// Commit the current transaction: write descriptor + data blocks +
    /// commit block to the circular log, then update the superblock.
    pub fn journal_commit_transaction(&self) -> FsResult<()> {
        // Take ownership of the handle.
        let handle_opt = {
            let mut handle = self.current_handle.lock();
            handle.take()
        };
        let mut handle = handle_opt.ok_or(FsError::InvalidArgument)?;
        handle.state = TransactionState::Committing;

        let sb = *self.superblock.lock();
        // The log starts at s_start (relative to journal area start).
        // If s_start is 0, start at block 1 (after the superblock).
        let mut log_pos = if sb.s_start == 0 { 1 } else { sb.s_start };
        let seq = handle.sequence;
        let dirty_blocks: Vec<&JournalBlock> =
            handle.blocks.iter().filter(|b| b.dirty).collect();

        let mut log_blocks_used: Vec<u32> = Vec::new();
        let mut checkpoint_pairs: Vec<(u64, u32)> = Vec::new();

        // Write descriptor block.
        let desc_block_num = log_pos;
        let mut desc_data = vec![0u8; self.block_size as usize];
        let desc_header = JournalHeader {
            h_magic: JBD2_MAGIC_NUMBER,
            h_blocktype: JBD2_DESCRIPTOR_BLOCK,
            h_sequence: seq,
        };
        let header_bytes = unsafe {
            core::slice::from_raw_parts(
                &desc_header as *const JournalHeader as *const u8,
                core::mem::size_of::<JournalHeader>(),
            )
        };
        desc_data[..header_bytes.len()].copy_from_slice(header_bytes);
        // Write tags for each dirty block.
        let mut tag_off = core::mem::size_of::<JournalHeader>();
        for (i, b) in dirty_blocks.iter().enumerate() {
            if tag_off + core::mem::size_of::<JournalBlockTag>() > desc_data.len() {
                break;
            }
            let is_last = i == dirty_blocks.len() - 1;
            let tag = JournalBlockTag {
                t_blocknr: b.fs_block as u32,
                t_flags: if is_last { JBD2_FLAG_LAST_TAG } else { 0 },
            };
            let tag_bytes = unsafe {
                core::slice::from_raw_parts(
                    &tag as *const JournalBlockTag as *const u8,
                    core::mem::size_of::<JournalBlockTag>(),
                )
            };
            desc_data[tag_off..tag_off + tag_bytes.len()].copy_from_slice(tag_bytes);
            tag_off += core::mem::size_of::<JournalBlockTag>();
        }
        self.write_log_block(desc_block_num, &desc_data)?;
        log_blocks_used.push(desc_block_num);
        log_pos = self.next_log_pos(log_pos);

        // Write data blocks.
        for b in &dirty_blocks {
            self.write_log_block(log_pos, &b.data)?;
            checkpoint_pairs.push((b.fs_block, log_pos));
            log_blocks_used.push(log_pos);
            log_pos = self.next_log_pos(log_pos);
        }

        // Write commit block.
        let commit_block_num = log_pos;
        let mut commit_data = vec![0u8; self.block_size as usize];
        let commit_header = JournalCommitHeader {
            h_magic: JBD2_MAGIC_NUMBER,
            h_blocktype: JBD2_COMMIT_BLOCK,
            h_sequence: seq,
            h_commit_sec: 0,
            h_commit_nsec: 0,
        };
        let commit_bytes = unsafe {
            core::slice::from_raw_parts(
                &commit_header as *const JournalCommitHeader as *const u8,
                core::mem::size_of::<JournalCommitHeader>(),
            )
        };
        commit_data[..commit_bytes.len()].copy_from_slice(commit_bytes);
        self.write_log_block(commit_block_num, &commit_data)?;
        log_blocks_used.push(commit_block_num);
        log_pos = self.next_log_pos(log_pos);

        // Update superblock: new s_start and s_sequence.
        {
            let mut sb = self.superblock.lock();
            sb.s_start = log_pos;
            sb.s_sequence = seq + 1;
        }
        self.write_superblock()?;

        // Record for checkpointing.
        {
            let mut cl = self.checkpoint_list.lock();
            cl.insert(seq, checkpoint_pairs);
        }
        {
            let mut cl = self.committed_log.lock();
            cl.insert(seq, log_blocks_used);
        }

        handle.state = TransactionState::Finished;
        Ok(())
    }

    /// Advance the log position, wrapping around the circular log.
    fn next_log_pos(&self, pos: u32) -> u32 {
        let next = pos + 1;
        if next >= self.count {
            1 // Wrap to block 1 (skip superblock at block 0)
        } else {
            next
        }
    }

    /// Checkpoint: write committed blocks to their final disk locations.
    pub fn journal_checkpoint(&self) -> FsResult<()> {
        let to_checkpoint: Vec<(u32, Vec<(u64, u32)>)> = {
            let cl = self.checkpoint_list.lock();
            cl.iter().map(|(k, v)| (*k, v.clone())).collect()
        };
        for (seq, pairs) in to_checkpoint {
            for (fs_block, log_block) in &pairs {
                let data = self.read_log_block(*log_block)?;
                self.device
                    .write_block(*fs_block, &data)
                    .map_err(|_| FsError::IoError)?;
            }
            // Remove from checkpoint list.
            {
                let mut cl = self.checkpoint_list.lock();
                cl.remove(&seq);
            }
            {
                let mut cl = self.committed_log.lock();
                cl.remove(&seq);
            }
        }
        Ok(())
    }

    /// Flush: commit + checkpoint everything.
    pub fn journal_flush(&self) -> FsResult<()> {
        // Commit if there's a running transaction.
        {
            let handle = self.current_handle.lock();
            if handle.is_some() {
                drop(handle);
                self.journal_commit_transaction()?;
            }
        }
        self.journal_checkpoint()?;
        Ok(())
    }

    /// Recover: replay the journal after a crash.
    /// Scans the log from s_start, replays descriptor + data blocks, then
    /// checkpoints.
    pub fn journal_recover(&self) -> FsResult<()> {
        let sb = *self.superblock.lock();
        if sb.s_start == 0 {
            // No recovery needed.
            return Ok(());
        }
        let mut log_pos = sb.s_start;
        let expected_seq = sb.s_sequence;
        let mut recovered: Vec<(u64, Vec<u8>)> = Vec::new();

        // Scan the log for complete transactions.
        loop {
            let data = self.read_log_block(log_pos)?;
            if data.len() < core::mem::size_of::<JournalHeader>() {
                break;
            }
            let header = unsafe {
                core::ptr::read_unaligned(data.as_ptr() as *const JournalHeader)
            };
            if header.h_magic != JBD2_MAGIC_NUMBER {
                break;
            }
            match header.h_blocktype {
                JBD2_DESCRIPTOR_BLOCK => {
                    // Parse tags and collect (fs_block, log_offset) pairs.
                    let mut tag_off = core::mem::size_of::<JournalHeader>();
                    let mut block_list: Vec<(u64, u32)> = Vec::new();
                    loop {
                        if tag_off + core::mem::size_of::<JournalBlockTag>() > data.len() {
                            break;
                        }
                        let tag = unsafe {
                            core::ptr::read_unaligned(
                                data.as_ptr().add(tag_off) as *const JournalBlockTag,
                            )
                        };
                        if tag.t_blocknr == 0 && tag.t_flags == 0 {
                            break;
                        }
                        let next_log = self.next_log_pos(log_pos);
                        block_list.push((tag.t_blocknr as u64, next_log));
                        tag_off += core::mem::size_of::<JournalBlockTag>();
                        if tag.t_flags & JBD2_FLAG_LAST_TAG != 0 {
                            break;
                        }
                    }
                    // Read data blocks following the descriptor.
                    for (fs_block, data_log_pos) in &block_list {
                        let block_data = self.read_log_block(*data_log_pos)?;
                        recovered.push((*fs_block, block_data));
                        log_pos = self.next_log_pos(*data_log_pos);
                    }
                }
                JBD2_COMMIT_BLOCK => {
                    // Transaction is complete — replay it.
                    for (fs_block, block_data) in &recovered {
                        self.device
                            .write_block(*fs_block, block_data)
                            .map_err(|_| FsError::IoError)?;
                    }
                    recovered.clear();
                    log_pos = self.next_log_pos(log_pos);
                    if header.h_sequence >= expected_seq {
                        break;
                    }
                }
                _ => {
                    // Unknown block type — stop recovery.
                    break;
                }
            }
        }
        // Reset the superblock to indicate a clean journal.
        {
            let mut sb_mut = self.superblock.lock();
            sb_mut.s_start = 0;
            sb_mut.s_sequence = expected_seq;
        }
        self.write_superblock()?;
        Ok(())
    }

    /// Get journal statistics for statfs.
    fn journal_stats(&self) -> (u64, u64) {
        let sb = *self.superblock.lock();
        let total = self.count as u64;
        let used = self.checkpoint_list.lock().len() as u64;
        (total, total.saturating_sub(used))
    }
}

// ============================================================================
// Jbd2FileSystem — wraps a Journal as a FileSystem (statfs + sync only)
// ============================================================================

/// JBD2 filesystem wrapper. The journal is a block-device layer, not a
/// standalone filesystem, so most FileSystem methods return `NotSupported`.
/// `statfs` returns journal statistics, and `sync` calls `journal_flush`.
#[derive(Debug)]
pub struct Jbd2FileSystem {
    journal: Arc<Journal>,
}

impl Jbd2FileSystem {
    /// Create a new JBD2 filesystem wrapper around a journal.
    pub fn new(device: Arc<dyn BlockDevice>, start: u64, count: u32) -> FsResult<Self> {
        let journal = Arc::new(Journal::new(device, start, count)?);
        Ok(Self { journal })
    }

    /// Get a reference to the underlying journal.
    pub fn journal(&self) -> &Arc<Journal> {
        &self.journal
    }
}

impl FileSystem for Jbd2FileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::RamFs // JBD2 is a journaling layer, not a standalone fs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let (total, free) = self.journal.journal_stats();
        Ok(FileSystemStats {
            total_blocks: total,
            free_blocks: free,
            available_blocks: free,
            total_inodes: 0,
            free_inodes: 0,
            block_size: self.journal.block_size,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn open(&self, _path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn read(&self, _inode: InodeNumber, _offset: u64, _buffer: &mut [u8]) -> FsResult<usize> {
        Err(FsError::NotSupported)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::NotSupported)
    }

    fn metadata(&self, _inode: InodeNumber) -> FsResult<FileMetadata> {
        Err(FsError::NotSupported)
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

    fn readdir(&self, _inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        Err(FsError::NotSupported)
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
        self.journal.journal_flush()
    }
}
