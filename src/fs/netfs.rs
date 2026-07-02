//! Network filesystem helper library.
//!
//! Provides a shared helper library for network-based filesystems (NFS, CIFS,
//! AFS, etc.) to perform read/write operations with caching and buffer
//! management.  Includes a page cache, an in-memory request/response
//! transport with a simulated server, and a helper trait for
//! protocol-specific implementations.

use crate::fs::{FsError, FsResult, InodeNumber};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, Ordering};
use spin::{Mutex, RwLock};

/// Default page size for the netfs page cache (4 KiB).
const DEFAULT_PAGE_SIZE: u64 = 4096;

/// Network filesystem I/O request.
#[derive(Debug, Clone)]
pub struct NetFsRequest {
    /// Request type (0 = read, 1 = write, 2 = invalidate).
    pub request_type: u32,
    /// Inode number the request targets.
    pub inode: InodeNumber,
    /// File offset.
    pub offset: u64,
    /// Length of I/O.
    pub length: u64,
    /// Data payload (for write requests).
    pub data: Vec<u8>,
}

/// Network filesystem I/O response.
#[derive(Debug, Clone)]
pub struct NetFsResponse {
    /// Status code (0 = success, non-zero = error).
    pub status: u32,
    /// Inode number the response is for.
    pub inode: InodeNumber,
    /// File offset.
    pub offset: u64,
    /// Data payload (for read responses).
    pub data: Vec<u8>,
}

/// Cache state for a page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheState {
    /// Data is cached and valid.
    Cached,
    /// Data is not cached.
    NotCached,
    /// Cache state unknown (needs revalidation).
    Unknown,
}

/// Page cache for network filesystem I/O.
/// Stores pages keyed by (inode, page_index).
pub struct NetFsPageCache {
    pages: Mutex<BTreeMap<(InodeNumber, u64), Vec<u8>>>,
    page_size: u64,
}

impl NetFsPageCache {
    /// Create a new page cache with the default page size.
    pub fn new() -> Self {
        Self {
            pages: Mutex::new(BTreeMap::new()),
            page_size: DEFAULT_PAGE_SIZE,
        }
    }

    /// Create a page cache with a custom page size.
    pub fn with_page_size(page_size: u64) -> Self {
        Self {
            pages: Mutex::new(BTreeMap::new()),
            page_size,
        }
    }

    /// Read a page from the cache. Returns None if not cached.
    pub fn read_page(&self, inode: InodeNumber, page_index: u64) -> Option<Vec<u8>> {
        let pages = self.pages.lock();
        pages.get(&(inode, page_index)).cloned()
    }

    /// Write a page to the cache.
    pub fn write_page(&self, inode: InodeNumber, page_index: u64, data: Vec<u8>) {
        let mut pages = self.pages.lock();
        pages.insert((inode, page_index), data);
    }

    /// Invalidate a cached page.
    pub fn invalidate(&self, inode: InodeNumber, page_index: u64) {
        let mut pages = self.pages.lock();
        pages.remove(&(inode, page_index));
    }

    /// Invalidate all pages for a given inode.
    pub fn invalidate_inode(&self, inode: InodeNumber) {
        let mut pages = self.pages.lock();
        let keys: Vec<(InodeNumber, u64)> = pages
            .keys()
            .filter(|(ino, _)| *ino == inode)
            .cloned()
            .collect();
        for key in keys {
            pages.remove(&key);
        }
    }

    /// Flush all cached pages (in a real implementation, this would write back
    /// dirty pages; here it just clears the cache).
    pub fn flush(&self) {
        self.pages.lock().clear();
    }

    /// Get the page size.
    pub fn page_size(&self) -> u64 {
        self.page_size
    }

    /// Check if a page is cached.
    pub fn is_cached(&self, inode: InodeNumber, page_index: u64) -> bool {
        self.pages.lock().contains_key(&(inode, page_index))
    }
}

/// Simulated server for the in-memory request/response transport.
/// Stores file data that can be read/written via requests.
pub struct SimServer {
    storage: RwLock<BTreeMap<InodeNumber, Vec<u8>>>,
}

impl SimServer {
    /// Create a new simulated server.
    pub fn new() -> Self {
        Self {
            storage: RwLock::new(BTreeMap::new()),
        }
    }

    /// Insert file data on the server.
    pub fn put_file(&self, inode: InodeNumber, data: Vec<u8>) {
        self.storage.write().insert(inode, data);
    }

    /// Handle a read request.
    fn handle_read(&self, req: &NetFsRequest) -> NetFsResponse {
        let storage = self.storage.read();
        match storage.get(&req.inode) {
            Some(data) => {
                let start = req.offset as usize;
                let end = core::cmp::min(start + req.length as usize, data.len());
                if start >= data.len() {
                    return NetFsResponse {
                        status: 0,
                        inode: req.inode,
                        offset: req.offset,
                        data: Vec::new(),
                    };
                }
                NetFsResponse {
                    status: 0,
                    inode: req.inode,
                    offset: req.offset,
                    data: data[start..end].to_vec(),
                }
            }
            None => NetFsResponse {
                status: 1, // ENOENT
                inode: req.inode,
                offset: req.offset,
                data: Vec::new(),
            },
        }
    }

    /// Handle a write request.
    fn handle_write(&self, req: &NetFsRequest) -> NetFsResponse {
        let mut storage = self.storage.write();
        let data = storage
            .entry(req.inode)
            .or_insert_with(Vec::new);
        let start = req.offset as usize;
        let end = start + req.data.len();
        if data.len() < end {
            data.resize(end, 0);
        }
        data[start..end].copy_from_slice(&req.data);
        NetFsResponse {
            status: 0,
            inode: req.inode,
            offset: req.offset,
            data: Vec::new(),
        }
    }

    /// Process a request and return a response.
    pub fn process(&self, req: &NetFsRequest) -> NetFsResponse {
        match req.request_type {
            0 => self.handle_read(req),
            1 => self.handle_write(req),
            2 => {
                // Invalidate: just remove the file
                self.storage.write().remove(&req.inode);
                NetFsResponse {
                    status: 0,
                    inode: req.inode,
                    offset: 0,
                    data: Vec::new(),
                }
            }
            _ => NetFsResponse {
                status: 22, // EINVAL
                inode: req.inode,
                offset: req.offset,
                data: Vec::new(),
            },
        }
    }
}

/// In-memory request/response transport with a simulated server.
pub struct NetFsRequestQueue {
    server: SimServer,
}

impl NetFsRequestQueue {
    /// Create a new request queue with a simulated server.
    pub fn new() -> Self {
        Self {
            server: SimServer::new(),
        }
    }

    /// Get a reference to the simulated server (for pre-populating data).
    pub fn server(&self) -> &SimServer {
        &self.server
    }

    /// Submit a request and get a response synchronously.
    pub fn submit(&self, req: &NetFsRequest) -> FsResult<NetFsResponse> {
        let resp = self.server.process(req);
        if resp.status != 0 {
            return Err(FsError::IoError);
        }
        Ok(resp)
    }
}

/// Network filesystem mount context.
pub struct NetFsContext {
    /// Page cache for I/O.
    pub cache: NetFsPageCache,
    /// Request queue for server communication.
    pub queue: NetFsRequestQueue,
    /// Mount options (key-value pairs encoded as string).
    pub mount_options: String,
    /// Connection state: true if connected.
    pub connected: AtomicBool,
}

impl NetFsContext {
    /// Create a new network filesystem context.
    pub fn new(mount_options: &str) -> Self {
        Self {
            cache: NetFsPageCache::new(),
            queue: NetFsRequestQueue::new(),
            mount_options: mount_options.to_string(),
            connected: AtomicBool::new(true),
        }
    }

    /// Check if the context is connected.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Disconnect.
    pub fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);
    }

    /// Reconnect.
    pub fn reconnect(&self) {
        self.connected.store(true, Ordering::SeqCst);
    }
}

/// Trait for network filesystem protocol helpers.
/// Protocol implementations (NFS, CIFS, etc.) implement this to provide
/// protocol-specific read/write coordination.
pub trait NetFsHelper {
    /// Begin a read operation. Called before fetching data from the server.
    fn begin_read(&self, inode: InodeNumber, offset: u64, count: u64) -> FsResult<()>;

    /// End a read operation. Called after data has been fetched.
    fn end_read(&self, inode: InodeNumber, offset: u64, data: &[u8]) -> FsResult<()>;

    /// Begin a write operation. Called before sending data to the server.
    fn begin_write(&self, inode: InodeNumber, offset: u64, count: u64) -> FsResult<()>;

    /// End a write operation. Called after data has been sent.
    fn end_write(&self, inode: InodeNumber, offset: u64, data: &[u8]) -> FsResult<()>;
}

/// Initialize the netfs subsystem.
pub fn init() -> FsResult<()> {
    Ok(())
}

/// Submit a read request with page-cache-first semantics.
/// Checks the page cache first; for cache misses, fetches from the server
/// and populates the cache.
pub fn netfs_submit_read(
    ctx: &NetFsContext,
    inode: InodeNumber,
    offset: u64,
    count: u64,
) -> FsResult<Vec<u8>> {
    if !ctx.is_connected() {
        return Err(FsError::IoError);
    }

    let page_size = ctx.cache.page_size();
    let mut result = Vec::with_capacity(count as usize);
    let mut cur = offset;
    let end = offset + count;

    while cur < end {
        let page_index = cur / page_size;
        let offset_in_page = cur % page_size;
        let bytes_to_end = end - cur;
        let bytes_in_page = page_size - offset_in_page;
        let chunk = core::cmp::min(bytes_to_end, bytes_in_page);

        // Try cache first
        if let Some(page_data) = ctx.cache.read_page(inode, page_index) {
            let start = offset_in_page as usize;
            let take = core::cmp::min(chunk as usize, page_data.len().saturating_sub(start));
            if take > 0 {
                result.extend_from_slice(&page_data[start..start + take]);
            } else {
                result.resize(result.len() + chunk as usize, 0);
            }
        } else {
            // Cache miss: fetch from server
            let req = NetFsRequest {
                request_type: 0, // read
                inode,
                offset: page_index * page_size,
                length: page_size,
                data: Vec::new(),
            };
            let resp = ctx.queue.submit(&req)?;
            let page_data = resp.data;

            // Cache the fetched page
            ctx.cache.write_page(inode, page_index, page_data.clone());

            // Extract the needed bytes
            let start = offset_in_page as usize;
            let take = core::cmp::min(chunk as usize, page_data.len().saturating_sub(start));
            if take > 0 {
                result.extend_from_slice(&page_data[start..start + take]);
            } else {
                result.resize(result.len() + chunk as usize, 0);
            }
        }

        cur += chunk;
    }

    Ok(result)
}

/// Submit a write request with page-cache write-through.
/// Writes data to the server and updates the page cache.
pub fn netfs_submit_write(
    ctx: &NetFsContext,
    inode: InodeNumber,
    offset: u64,
    data: &[u8],
) -> FsResult<usize> {
    if !ctx.is_connected() {
        return Err(FsError::IoError);
    }

    // Send write to server
    let req = NetFsRequest {
        request_type: 1, // write
        inode,
        offset,
        length: data.len() as u64,
        data: data.to_vec(),
    };
    let _resp = ctx.queue.submit(&req)?;

    // Update page cache
    let page_size = ctx.cache.page_size();
    let mut cur = offset;
    let mut data_pos = 0usize;

    while data_pos < data.len() {
        let page_index = cur / page_size;
        let offset_in_page = cur % page_size;
        let bytes_in_page = page_size - offset_in_page;
        let remaining = data.len() - data_pos;
        let chunk = core::cmp::min(remaining as u64, bytes_in_page) as usize;

        // Read existing page from cache (or server) and update
        let mut page_data = ctx.cache.read_page(inode, page_index).unwrap_or_else(|| {
            // Fetch from server
            let req = NetFsRequest {
                request_type: 0,
                inode,
                offset: page_index * page_size,
                length: page_size,
                data: Vec::new(),
            };
            ctx.queue.submit(&req).map(|r| r.data).unwrap_or_default()
        });

        // Ensure page is large enough
        let needed = offset_in_page as usize + chunk;
        if page_data.len() < needed {
            page_data.resize(needed, 0);
        }

        page_data[offset_in_page as usize..offset_in_page as usize + chunk]
            .copy_from_slice(&data[data_pos..data_pos + chunk]);
        ctx.cache.write_page(inode, page_index, page_data);

        cur += chunk as u64;
        data_pos += chunk;
    }

    Ok(data.len())
}

/// Legacy API: submit a netfs read request.
pub fn submit_read(req: &NetFsRequest) -> FsResult<usize> {
    // Without a context, we can't do cache-first reads.
    // Create a temporary context and submit.
    let ctx = NetFsContext::new("legacy");
    if !ctx.is_connected() {
        return Err(FsError::IoError);
    }
    let resp = ctx.queue.submit(req)?;
    Ok(resp.data.len())
}
