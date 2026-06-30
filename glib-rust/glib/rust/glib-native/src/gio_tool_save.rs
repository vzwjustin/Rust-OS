//! gio-tool-save matching `gio/gio-tool-save.c`.
//!
//! Read from standard input and save to a destination file.

use crate::gfile::File;
use crate::goutputstream::OutputStream;
use crate::prelude::*;

/// Options for save.
#[derive(Clone, Debug, Default)]
pub struct SaveOptions {
    pub backup: bool,
    pub create: bool,
    pub append: bool,
    pub private: bool,
    pub replace_dest: bool,
    pub print_etag: bool,
    pub etag: Option<String>,
}

/// Save `data` to `file` using the given options.
pub fn save_data(file: &File, data: &[u8], opts: &SaveOptions) -> Result<Option<String>, String> {
    let flags = FileCreateOptions {
        backup: opts.backup,
        private: opts.private,
        replace_destination: opts.replace_dest,
    };
    let stream: OutputStream = if opts.create {
        file.create(flags.to_create_flags(), None)
    } else {
        // Append mode falls back to replace in the no_std stub.
        file.replace(
            opts.etag.as_deref(),
            opts.backup,
            flags.to_create_flags(),
            None,
        )
    }
    .map_err(|e| e.message().to_owned())?;
    let (_, write_err) = stream
        .write_all(data, None)
        .map_err(|e| e.message().to_owned())?;
    if let Some(e) = write_err {
        return Err(e.message().to_owned());
    }
    stream.close(None).map_err(|e| e.message().to_owned())?;
    let etag = if opts.print_etag {
        Some(format!("{:x}", simple_hash(data)))
    } else {
        None
    };
    Ok(etag)
}

/// Simple etag stub (CRC-like hash).
fn simple_hash(data: &[u8]) -> u32 {
    data.iter()
        .fold(0u32, |acc, b| acc.wrapping_mul(31).wrapping_add(*b as u32))
}

/// Entry point for `gio save`. `stdin_data` supplies bytes when no OS stdin exists.
pub fn run_with_stdin(args: &[&str], stdin_data: &[u8]) -> i32 {
    let (opts, positional) = match parse_options(args) {
        Ok(v) => v,
        Err(e) if e == "help" => return 0,
        Err(_) => return 1,
    };
    if positional.len() != 1 {
        return 1;
    }
    let file = File::new_for_commandline_arg(positional[0]);
    match save_data(&file, stdin_data, &opts) {
        Ok(Some(_etag)) => {
            gwarn!("Etag: {etag}");
            0
        }
        Ok(None) => 0,
        Err(_msg) => {
            gwarn!("{msg}");
            2
        }
    }
}

/// Entry point for `gio save` with empty stdin (stub).
pub fn run(args: &[&str]) -> i32 {
    run_with_stdin(args, &[])
}

fn parse_options<'a>(args: &'a [&'a str]) -> Result<(SaveOptions, Vec<&'a str>), String> {
    let mut opts = SaveOptions::default();
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-b" | "--backup" => opts.backup = true,
            "-c" | "--create" => opts.create = true,
            "-a" | "--append" => opts.append = true,
            "-p" | "--private" => opts.private = true,
            "-u" | "--unlink" => opts.replace_dest = true,
            "-v" | "--print-etag" => opts.print_etag = true,
            "-e" | "--etag" => {
                i += 1;
                opts.etag = args.get(i).map(|s| (*s).to_owned());
            }
            "-h" | "--help" => return Err("help".into()),
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => positional.push(other),
        }
        i += 1;
    }
    Ok((opts, positional))
}

/// Helper bridging save flags to [`FileCreateFlags`].
pub struct FileCreateOptions {
    pub backup: bool,
    pub private: bool,
    pub replace_destination: bool,
}

impl FileCreateOptions {
    fn to_create_flags(&self) -> crate::gfile::FileCreateFlags {
        use crate::gfile::FileCreateFlags;
        let mut f = FileCreateFlags::None;
        if self.private {
            f = FileCreateFlags::Private;
        }
        if self.replace_destination {
            f = FileCreateFlags::ReplaceDestination;
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfile::{register_file_platform, FileCreateFlags, FilePlatform};
    use crate::ginputstream::InputStream;
    use crate::gioerror::IOErrorEnum;
    use crate::goutputstream::{MemoryOutputStream, OutputStream};

    struct SavePlatform;
    impl FilePlatform for SavePlatform {
        fn read(&self, _: &str) -> Result<InputStream, crate::error::Error> {
            Err(crate::error::Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::NotSupported.to_code(),
                "",
            ))
        }
        fn create(&self, _: &str, _: FileCreateFlags) -> Result<OutputStream, crate::error::Error> {
            Ok(OutputStream::from(MemoryOutputStream::new_resizable()))
        }
        fn replace(
            &self,
            _: &str,
            _: Option<&str>,
            _: bool,
            _: FileCreateFlags,
        ) -> Result<OutputStream, crate::error::Error> {
            Ok(OutputStream::from(MemoryOutputStream::new_resizable()))
        }
        fn query_exists(&self, _: &str) -> bool {
            false
        }
        fn query_info(
            &self,
            _: &str,
            _: &str,
            _: crate::gfile::FileQueryInfoFlags,
        ) -> Result<crate::gfile::FileInfo, crate::error::Error> {
            Err(crate::error::Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::NotFound.to_code(),
                "",
            ))
        }
        fn delete(&self, _: &str) -> Result<(), crate::error::Error> {
            Err(crate::error::Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::NotSupported.to_code(),
                "",
            ))
        }
        fn trash(&self, _: &str) -> Result<(), crate::error::Error> {
            Err(crate::error::Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::NotSupported.to_code(),
                "",
            ))
        }
    }
    static SAVE_PLATFORM: SavePlatform = SavePlatform;

    #[test]
    fn save_writes_data() {
        register_file_platform(&SAVE_PLATFORM);
        let f = File::new_for_path("/out.txt");
        let etag = save_data(
            &f,
            b"hello",
            &SaveOptions {
                print_etag: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(etag.is_some());
    }

    #[test]
    fn missing_destination_fails() {
        assert_eq!(run(&[]), 1);
    }

    #[test]
    fn parse_backup_flag() {
        let (opts, pos) = parse_options(&["-b", "/tmp/x"]).unwrap();
        assert!(opts.backup);
        assert_eq!(pos, vec!["/tmp/x"]);
    }
}
