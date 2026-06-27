//! Command-line option parser matching `goption.h` / `goption.c`.
//!
//! [`OptionContext`] collects [`OptionGroup`]s and their [`OptionEntry`] tables,
//! then parses `argv`-style argument vectors in place.

use crate::error::Error;
use crate::prelude::*;
use crate::quark::{quark_from_static_string, Quark};
use core::ffi::c_void;
use core::ptr;

/// Sentinel long name that collects non-option arguments (`G_OPTION_REMAINING`).
pub const OPTION_REMAINING: &str = "";

/// Error domain quark for option parsing (`G_OPTION_ERROR` / `g_option_error_quark`).
pub fn option_error_quark() -> Quark {
    quark_from_static_string(Some("g-option-context-error-quark"))
}

/// Error codes returned by option parsing (`GOptionError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OptionError {
    /// An option was not known to the parser.
    UnknownOption = 0,
    /// A value could not be parsed.
    BadValue = 1,
    /// An [`OptionArgFunc`] callback failed.
    Failed = 2,
}

/// Bitfield of option flags (`GOptionFlags`).
pub type OptionFlags = i32;

/// Flags modifying individual options (`GOptionFlags` constants).
pub mod option_flags {
    /// No flags.
    pub const NONE: i32 = 0;
    /// The option does not appear in `--help` output.
    pub const HIDDEN: i32 = 1 << 0;
    /// The option appears in the main section of `--help` output.
    pub const IN_MAIN: i32 = 1 << 1;
    /// For [`super::OptionArg::None`], store `false` instead of `true`.
    pub const REVERSE: i32 = 1 << 2;
    /// For [`super::OptionArg::Callback`], the callback takes no argument.
    pub const NO_ARG: i32 = 1 << 3;
    /// For [`super::OptionArg::Callback`], the argument is optional.
    pub const OPTIONAL_ARG: i32 = 1 << 5;
    /// Disable automatic `groupname-` prefix on conflicting long names.
    pub const NOALIAS: i32 = 1 << 6;
}

/// Argument type for an option entry (`GOptionArg`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum OptionArg {
    /// No extra argument (boolean flag).
    None = 0,
    /// UTF-8 string argument.
    String = 1,
    /// Integer argument.
    Int = 2,
    /// Callback argument parser.
    Callback = 3,
    /// Filename argument (stored as UTF-8 on Unix).
    Filename = 4,
    /// Repeated string arguments collected into an array.
    StringArray = 5,
    /// Repeated filename arguments collected into an array.
    FilenameArray = 6,
    /// Floating-point argument.
    Double = 7,
    /// 64-bit integer argument.
    Int64 = 8,
}

/// Callback for [`OptionArg::Callback`] options (`GOptionArgFunc`).
pub type OptionArgFunc =
    fn(option_name: &str, value: Option<&str>, user_data: *mut c_void) -> Result<(), Error>;

/// A single option definition (`GOptionEntry`).
#[derive(Clone)]
pub struct OptionEntry {
    /// Long option name (`--long_name`), [`OPTION_REMAINING`], or `None` as terminator.
    pub long_name: Option<&'static str>,
    /// Short option character, or `'\0'` when absent.
    pub short_name: char,
    /// Bitfield of [`option_flags`] values.
    pub flags: i32,
    /// Expected argument type.
    pub arg: OptionArg,
    /// Storage pointer or callback function pointer.
    pub arg_data: *mut c_void,
    /// Help text for the option.
    pub description: Option<&'static str>,
    /// Placeholder name for the option argument in `--help`.
    pub arg_description: Option<&'static str>,
}

impl OptionEntry {
    /// Null terminator for a C-style entry array (`G_OPTION_ENTRY_NULL`).
    pub const NULL: Self = Self {
        long_name: None,
        short_name: '\0',
        flags: 0,
        arg: OptionArg::None,
        arg_data: ptr::null_mut(),
        description: None,
        arg_description: None,
    };

    /// Returns `true` when this entry marks the end of a C-style array.
    pub fn is_null(&self) -> bool {
        self.long_name.is_none()
    }
}

/// Option group (`GOptionGroup`).
pub struct OptionGroup {
    name: Option<String>,
    _description: Option<String>,
    _help_description: Option<String>,
    user_data: *mut c_void,
    /// Entries registered in this group.
    pub entries: Vec<OptionEntry>,
}

impl OptionGroup {
    /// Creates a new option group (`g_option_group_new`).
    pub fn new(
        name: Option<&str>,
        description: Option<&str>,
        help_description: Option<&str>,
        user_data: *mut c_void,
    ) -> Self {
        Self {
            name: name.map(str::to_owned),
            _description: description.map(str::to_owned),
            _help_description: help_description.map(str::to_owned),
            user_data,
            entries: Vec::new(),
        }
    }

    /// Adds entries to the group (`g_option_group_add_entries`).
    pub fn add_entries(&mut self, entries: &[OptionEntry]) {
        for entry in entries {
            if entry.is_null() {
                break;
            }
            let mut entry = entry.clone();
            validate_entry(&mut entry, self.name.as_deref());
            self.entries.push(entry);
        }
    }

    /// Number of entries registered in the group.
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

/// Parser context (`GOptionContext`).
pub struct OptionContext {
    groups: Vec<OptionGroup>,
    main_group: Option<OptionGroup>,
    _parameter_string: Option<String>,
    help_enabled: bool,
    ignore_unknown: bool,
    changes: Vec<Change>,
    pending_removals: Vec<PendingRemoval>,
}

struct Change {
    arg_type: OptionArg,
    arg_data: *mut c_void,
    prev_bool: bool,
    prev_int: i32,
    prev_string: String,
    had_string: bool,
    prev_array_len: usize,
    allocated_array: Vec<String>,
}

struct PendingRemoval {
    index: usize,
    short_rewrite: Option<String>,
}

struct OptionParseContext<'a> {
    context: &'a mut OptionContext,
    entries: &'a [OptionEntry],
    user_data: *mut c_void,
    argv: &'a [String],
}

/// Creates a new option context (`g_option_context_new`).
pub fn option_context_new(parameter_string: Option<&str>) -> OptionContext {
    let parameter_string = parameter_string.and_then(|s| {
        if s.is_empty() {
            None
        } else {
            Some(s.to_owned())
        }
    });

    OptionContext {
        groups: Vec::new(),
        main_group: None,
        _parameter_string: parameter_string,
        help_enabled: true,
        ignore_unknown: false,
        changes: Vec::new(),
        pending_removals: Vec::new(),
    }
}

impl OptionContext {
    /// Frees the context (`g_option_context_free`).
    pub fn free(self) {}

    /// Enables or disables automatic `--help` handling (`g_option_context_set_help_enabled`).
    pub fn set_help_enabled(&mut self, help_enabled: bool) {
        self.help_enabled = help_enabled;
    }

    /// Returns whether automatic `--help` generation is enabled.
    pub fn get_help_enabled(&self) -> bool {
        self.help_enabled
    }

    /// Sets whether unknown options are ignored (`g_option_context_set_ignore_unknown_options`).
    pub fn set_ignore_unknown_options(&mut self, ignore_unknown: bool) {
        self.ignore_unknown = ignore_unknown;
    }

    /// Returns whether unknown options are ignored.
    pub fn get_ignore_unknown_options(&self) -> bool {
        self.ignore_unknown
    }

    /// Convenience helper that ensures a main group exists and adds entries
    /// (`g_option_context_add_main_entries`).
    pub fn add_main_entries(&mut self, entries: &[OptionEntry], _translation_domain: Option<&str>) {
        if self.main_group.is_none() {
            self.main_group = Some(OptionGroup::new(None, None, None, ptr::null_mut()));
        }
        if let Some(main) = self.main_group.as_mut() {
            main.add_entries(entries);
        }
    }

    /// Adds a group to the context (`g_option_context_add_group`).
    pub fn add_group(&mut self, group: OptionGroup) {
        self.groups.push(group);
    }

    /// Parses command-line arguments (`g_option_context_parse`).
    ///
    /// `argv[0]` is treated as the program name. Parsed options are removed and
    /// the vector is compacted on success. On failure, `argv` is left unchanged
    /// and parsed values are reverted.
    pub fn parse(&mut self, argv: &mut Vec<String>) -> Result<(), Error> {
        let original = argv.clone();
        self.changes.clear();
        self.pending_removals.clear();

        if argv.is_empty() {
            return Ok(());
        }

        let parse_result = self.parse_inner(argv);
        if parse_result.is_err() {
            *argv = original;
        }
        parse_result
    }

    fn parse_inner(&mut self, argv: &mut Vec<String>) -> Result<(), Error> {
        let mut stop_parsing = false;
        let mut has_unknown = false;
        let mut separator_pos: Option<usize> = None;

        let mut index = 1;
        while index < argv.len() {
            let arg = argv[index].clone();
            let mut parsed = false;

            if arg.starts_with('-') && arg.len() > 1 && !stop_parsing {
                if let Some(long) = arg.strip_prefix("--") {
                    if long.is_empty() {
                        separator_pos = Some(index);
                        stop_parsing = true;
                        index += 1;
                        continue;
                    }

                    if self.help_enabled && help_option_requested(long) {
                        return Err(fail_parse(
                            self,
                            Error::new(
                                option_error_quark(),
                                OptionError::Failed as i32,
                                "help requested",
                            ),
                        ));
                    }

                    if let Some(main) = self.main_group.as_ref() {
                        let entries = main.entries.clone();
                        let user_data = main.user_data;
                        let mut parse = OptionParseContext {
                            context: self,
                            entries: &entries,
                            user_data,
                            argv,
                        };
                        if parse_long_option(&mut parse, &mut index, long, false, &mut parsed)? {
                            index += 1;
                            continue;
                        }
                    }

                    for group_idx in 0..self.groups.len() {
                        let entries = self.groups[group_idx].entries.clone();
                        let user_data = self.groups[group_idx].user_data;
                        let mut parse = OptionParseContext {
                            context: self,
                            entries: &entries,
                            user_data,
                            argv,
                        };
                        if parse_long_option(&mut parse, &mut index, long, false, &mut parsed)? {
                            break;
                        }
                    }

                    if !parsed {
                        if let Some(dash) = long.find('-') {
                            let (group_name, option) = long.split_at(dash);
                            let option = &option[1..];
                            for group_idx in 0..self.groups.len() {
                                if self.groups[group_idx].name.as_deref() == Some(group_name) {
                                    let entries = self.groups[group_idx].entries.clone();
                                    let user_data = self.groups[group_idx].user_data;
                                    let mut parse = OptionParseContext {
                                        context: self,
                                        entries: &entries,
                                        user_data,
                                        argv,
                                    };
                                    parse_long_option(
                                        &mut parse,
                                        &mut index,
                                        option,
                                        true,
                                        &mut parsed,
                                    )?;
                                    break;
                                }
                            }
                        }
                    }

                    if self.ignore_unknown && parsed {
                        index += 1;
                        continue;
                    }

                    if !parsed {
                        has_unknown = true;
                        if !self.ignore_unknown {
                            return Err(fail_parse(
                                self,
                                Error::new(
                                    option_error_quark(),
                                    OptionError::UnknownOption as i32,
                                    format!("Unknown option {arg}"),
                                ),
                            ));
                        }
                    }
                } else {
                    let short_chars: Vec<char> = arg[1..].chars().collect();
                    let mut new_index = index;
                    let mut nulled = vec![false; short_chars.len()];

                    for (pos, ch) in short_chars.iter().enumerate() {
                        if self.help_enabled
                            && (*ch == '?' || (*ch == 'h' && !context_has_h_entry(self)))
                        {
                            return Err(fail_parse(
                                self,
                                Error::new(
                                    option_error_quark(),
                                    OptionError::Failed as i32,
                                    "help requested",
                                ),
                            ));
                        }

                        parsed = false;
                        if let Some(main) = self.main_group.as_ref() {
                            let entries = main.entries.clone();
                            let user_data = main.user_data;
                            let mut parse = OptionParseContext {
                                context: self,
                                entries: &entries,
                                user_data,
                                argv,
                            };
                            parse_short_option(
                                &mut parse,
                                index,
                                &mut new_index,
                                *ch,
                                &mut parsed,
                            )?;
                        }

                        if !parsed {
                            for group_idx in 0..self.groups.len() {
                                let entries = self.groups[group_idx].entries.clone();
                                let user_data = self.groups[group_idx].user_data;
                                let mut parse = OptionParseContext {
                                    context: self,
                                    entries: &entries,
                                    user_data,
                                    argv,
                                };
                                parse_short_option(
                                    &mut parse,
                                    index,
                                    &mut new_index,
                                    *ch,
                                    &mut parsed,
                                )?;
                                if parsed {
                                    break;
                                }
                            }
                        }

                        if self.ignore_unknown && parsed {
                            nulled[pos] = true;
                        } else if self.ignore_unknown {
                            continue;
                        } else if !parsed {
                            return Err(fail_parse(
                                self,
                                Error::new(
                                    option_error_quark(),
                                    OptionError::UnknownOption as i32,
                                    format!("Unknown option {arg}"),
                                ),
                            ));
                        }
                    }

                    if self.ignore_unknown {
                        let remaining: String = short_chars
                            .iter()
                            .zip(nulled.iter())
                            .filter_map(|(ch, &removed)| if removed { None } else { Some(*ch) })
                            .collect();
                        self.pending_removals.push(PendingRemoval {
                            index,
                            short_rewrite: if remaining.is_empty() {
                                None
                            } else {
                                Some(format!("-{remaining}"))
                            },
                        });
                        index = new_index + 1;
                        continue;
                    }

                    if parsed {
                        index = new_index + 1;
                        continue;
                    }
                }
            } else {
                if let Some(main) = self.main_group.as_ref() {
                    let entries = main.entries.clone();
                    let user_data = main.user_data;
                    let mut parse = OptionParseContext {
                        context: self,
                        entries: &entries,
                        user_data,
                        argv,
                    };
                    parse_remaining_arg(&mut parse, &mut index, &mut parsed)?;
                }

                if !parsed && (has_unknown || arg.starts_with('-')) {
                    separator_pos = None;
                }
            }

            index += 1;
        }

        if let Some(pos) = separator_pos {
            self.mark_removed(pos);
        }

        apply_removals(self, argv);
        Ok(())
    }

    fn mark_removed(&mut self, index: usize) {
        self.pending_removals.push(PendingRemoval {
            index,
            short_rewrite: None,
        });
    }
}

/// Creates a new option group (`g_option_group_new`).
pub fn option_group_new(
    name: Option<&str>,
    description: Option<&str>,
    help_description: Option<&str>,
    user_data: *mut c_void,
) -> OptionGroup {
    OptionGroup::new(name, description, help_description, user_data)
}

fn help_option_requested(long: &str) -> bool {
    long == "help" || long == "help-all" || long.starts_with("help-")
}

fn validate_entry(entry: &mut OptionEntry, #[allow(unused_variables)] group_name: Option<&str>) {
    if entry.short_name == '-' || (entry.short_name != '\0' && !entry.short_name.is_ascii_graphic())
    {
        gwarn!(
            "warning: ignoring invalid short option '{}' in entry {:?}:{}",
            entry.short_name,
            group_name.unwrap_or("main"),
            entry.long_name.unwrap_or("")
        );
        entry.short_name = '\0';
    }

    if entry.arg != OptionArg::None && (entry.flags & option_flags::REVERSE) != 0 {
        gwarn!(
            "warning: ignoring reverse flag on option of arg-type {:?} in entry {:?}:{}",
            entry.arg,
            group_name.unwrap_or("main"),
            entry.long_name.unwrap_or("")
        );
        entry.flags &= !option_flags::REVERSE;
    }

    if entry.arg != OptionArg::Callback
        && (entry.flags & (option_flags::NO_ARG | option_flags::OPTIONAL_ARG)) != 0
    {
        gwarn!(
            "warning: ignoring no-arg or optional-arg flags on option of arg-type {:?} in entry {:?}:{}",
            entry.arg,
            group_name.unwrap_or("main"),
            entry.long_name.unwrap_or("")
        );
        entry.flags &= !(option_flags::NO_ARG | option_flags::OPTIONAL_ARG);
    }
}

fn no_arg(entry: &OptionEntry) -> bool {
    entry.arg == OptionArg::None
        || (entry.arg == OptionArg::Callback && (entry.flags & option_flags::NO_ARG) != 0)
}

fn optional_arg(entry: &OptionEntry) -> bool {
    entry.arg == OptionArg::Callback && (entry.flags & option_flags::OPTIONAL_ARG) != 0
}

fn context_has_h_entry(context: &OptionContext) -> bool {
    if let Some(main) = &context.main_group {
        if main.entries.iter().any(|e| e.short_name == 'h') {
            return true;
        }
    }
    context
        .groups
        .iter()
        .flat_map(|g| g.entries.iter())
        .any(|e| e.short_name == 'h')
}

fn get_change(
    context: &mut OptionContext,
    arg_type: OptionArg,
    arg_data: *mut c_void,
) -> &mut Change {
    if let Some(pos) = context
        .changes
        .iter()
        .position(|change| change.arg_data == arg_data)
    {
        return &mut context.changes[pos];
    }

    context.changes.push(Change {
        arg_type,
        arg_data,
        prev_bool: false,
        prev_int: 0,
        prev_string: String::new(),
        had_string: false,
        prev_array_len: 0,
        allocated_array: Vec::new(),
    });

    let last = context.changes.len() - 1;
    &mut context.changes[last]
}

fn parse_int(option_name: &str, value: &str) -> Result<i32, Error> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(Error::new(
            option_error_quark(),
            OptionError::BadValue as i32,
            format!("Cannot parse integer value \"{value}\" for {option_name}"),
        ));
    }

    let (sign, digits) = if let Some(rest) = trimmed.strip_prefix('-') {
        (-1i64, rest)
    } else if let Some(rest) = trimmed.strip_prefix('+') {
        (1, rest)
    } else {
        (1, trimmed)
    };

    let number = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        parse_digits(hex, 16)?
    } else if digits.starts_with('0') && digits.len() > 1 {
        parse_digits(digits, 8)?
    } else {
        parse_digits(digits, 10)?
    };

    let signed = sign.saturating_mul(number);
    i32::try_from(signed).map_err(|_| {
        Error::new(
            option_error_quark(),
            OptionError::BadValue as i32,
            format!("Integer value \"{value}\" for {option_name} out of range"),
        )
    })
}

fn parse_digits(value: &str, radix: u32) -> Result<i64, Error> {
    i64::from_str_radix(value, radix).map_err(|_| {
        Error::new(
            option_error_quark(),
            OptionError::BadValue as i32,
            format!("Cannot parse integer value \"{value}\""),
        )
    })
}

fn parse_arg(
    context: &mut OptionContext,
    user_data: *mut c_void,
    entry: &OptionEntry,
    value: Option<&str>,
    option_name: &str,
) -> Result<(), Error> {
    match entry.arg {
        OptionArg::None => {
            let change = get_change(context, OptionArg::None, entry.arg_data);
            unsafe {
                let target = entry.arg_data as *mut bool;
                if change.allocated_array.is_empty() && !change.had_string {
                    change.prev_bool = *target;
                }
                *target = (entry.flags & option_flags::REVERSE) == 0;
            }
        }
        OptionArg::String | OptionArg::Filename => {
            let value = value.ok_or_else(|| missing_arg(option_name))?;
            let change = get_change(context, entry.arg, entry.arg_data);
            unsafe {
                let target = entry.arg_data as *mut String;
                if !change.had_string {
                    change.prev_string = (*target).clone();
                    change.had_string = true;
                }
                *target = value.to_owned();
            }
        }
        OptionArg::StringArray | OptionArg::FilenameArray => {
            let value = value.ok_or_else(|| missing_arg(option_name))?;
            let change = get_change(context, entry.arg, entry.arg_data);
            if change.allocated_array.is_empty() {
                unsafe {
                    let target = entry.arg_data as *mut Vec<String>;
                    change.prev_array_len = (*target).len();
                }
            }
            change.allocated_array.push(value.to_owned());
            write_string_array(entry.arg_data, &change.allocated_array);
        }
        OptionArg::Int => {
            let value = value.ok_or_else(|| missing_arg(option_name))?;
            let parsed = parse_int(option_name, value)?;
            let change = get_change(context, OptionArg::Int, entry.arg_data);
            unsafe {
                let target = entry.arg_data as *mut i32;
                if change.allocated_array.is_empty() && !change.had_string {
                    change.prev_int = *target;
                }
                *target = parsed;
            }
        }
        OptionArg::Callback => {
            let callback: OptionArgFunc = unsafe { core::mem::transmute(entry.arg_data) };
            let arg_value = if optional_arg(entry) || no_arg(entry) {
                value
            } else {
                Some(value.ok_or_else(|| missing_arg(option_name))?)
            };
            callback(option_name, arg_value, user_data).map_err(|err| {
                if err.domain() == option_error_quark() {
                    err
                } else {
                    Error::new(
                        option_error_quark(),
                        OptionError::Failed as i32,
                        format!("Error parsing option {option_name}"),
                    )
                }
            })?;
        }
        OptionArg::Double | OptionArg::Int64 => {
            return Err(Error::new(
                option_error_quark(),
                OptionError::Failed as i32,
                format!("Unsupported option argument type for {option_name}"),
            ));
        }
    }

    Ok(())
}

fn write_string_array(arg_data: *mut c_void, values: &[String]) {
    unsafe {
        let target = arg_data as *mut Vec<String>;
        *target = values.to_vec();
    }
}

fn missing_arg(option_name: &str) -> Error {
    Error::new(
        option_error_quark(),
        OptionError::BadValue as i32,
        format!("Missing argument for {option_name}"),
    )
}

fn parse_short_option(
    parse: &mut OptionParseContext<'_>,
    idx: usize,
    new_idx: &mut usize,
    ch: char,
    parsed: &mut bool,
) -> Result<(), Error> {
    for entry in parse.entries {
        if entry.short_name != ch {
            continue;
        }

        let option_name = format!("-{ch}");
        let value = if no_arg(entry) {
            None
        } else if *new_idx > idx {
            return Err(fail_parse(
                parse.context,
                Error::new(
                    option_error_quark(),
                    OptionError::Failed as i32,
                    format!("Error parsing option {option_name}"),
                ),
            ));
        } else if idx + 1 < parse.argv.len() {
            if optional_arg(entry) && parse.argv[idx + 1].starts_with('-') {
                None
            } else {
                *new_idx = idx + 1;
                Some(parse.argv[idx + 1].as_str())
            }
        } else if optional_arg(entry) {
            None
        } else {
            return Err(fail_parse(parse.context, missing_arg(&option_name)));
        };

        parse_arg(parse.context, parse.user_data, entry, value, &option_name)?;
        parse.context.mark_removed(idx);
        if *new_idx > idx {
            parse.context.mark_removed(*new_idx);
        }
        *parsed = true;
    }
    Ok(())
}

fn parse_long_option(
    parse: &mut OptionParseContext<'_>,
    idx: &mut usize,
    arg: &str,
    aliased: bool,
    parsed: &mut bool,
) -> Result<bool, Error> {
    for entry in parse.entries {
        if aliased && (entry.flags & option_flags::NOALIAS) != 0 {
            continue;
        }

        let Some(long_name) = entry.long_name else {
            continue;
        };

        if no_arg(entry) && arg == long_name {
            let option_name = format!("--{long_name}");
            parse.context.mark_removed(*idx);
            parse_arg(parse.context, parse.user_data, entry, None, &option_name)?;
            *parsed = true;
            return Ok(true);
        }

        if arg == long_name || arg.starts_with(&format!("{long_name}=")) {
            let option_name = format!("--{long_name}");
            let value = if let Some(rest) = arg.strip_prefix(&format!("{long_name}=")) {
                parse.context.mark_removed(*idx);
                Some(rest)
            } else if *idx + 1 < parse.argv.len() {
                if optional_arg(entry) && parse.argv[*idx + 1].starts_with('-') {
                    parse.context.mark_removed(*idx);
                    parse_arg(parse.context, parse.user_data, entry, None, &option_name)?;
                    *parsed = true;
                    return Ok(true);
                }
                parse.context.mark_removed(*idx);
                *idx += 1;
                parse.context.mark_removed(*idx);
                Some(parse.argv[*idx].as_str())
            } else if optional_arg(entry) {
                parse.context.mark_removed(*idx);
                parse_arg(parse.context, parse.user_data, entry, None, &option_name)?;
                *parsed = true;
                return Ok(true);
            } else {
                return Err(fail_parse(parse.context, missing_arg(&option_name)));
            };

            parse_arg(parse.context, parse.user_data, entry, value, &option_name)?;
            *parsed = true;
            return Ok(true);
        }
    }

    Ok(false)
}

fn parse_remaining_arg(
    parse: &mut OptionParseContext<'_>,
    idx: &mut usize,
    parsed: &mut bool,
) -> Result<(), Error> {
    for entry in parse.entries {
        let Some(long_name) = entry.long_name else {
            continue;
        };
        if !long_name.is_empty() {
            continue;
        }

        parse.context.mark_removed(*idx);
        parse_arg(
            parse.context,
            parse.user_data,
            entry,
            Some(&parse.argv[*idx]),
            "",
        )?;
        *parsed = true;
        return Ok(());
    }
    Ok(())
}

fn apply_removals(context: &mut OptionContext, argv: &mut Vec<String>) {
    for removal in &context.pending_removals {
        if let Some(replacement) = &removal.short_rewrite {
            argv[removal.index] = replacement.clone();
        } else {
            argv[removal.index].clear();
        }
    }

    let mut out = 0;
    for i in 0..argv.len() {
        if !argv[i].is_empty() {
            if out != i {
                argv[out] = core::mem::take(&mut argv[i]);
            }
            out += 1;
        }
    }
    argv.truncate(out);
    context.pending_removals.clear();
}

fn revert_changes(context: &mut OptionContext) {
    for change in context.changes.drain(..).rev() {
        match change.arg_type {
            OptionArg::None => unsafe {
                *(change.arg_data as *mut bool) = change.prev_bool;
            },
            OptionArg::Int => unsafe {
                *(change.arg_data as *mut i32) = change.prev_int;
            },
            OptionArg::String | OptionArg::Filename => unsafe {
                if change.had_string {
                    *(change.arg_data as *mut String) = change.prev_string;
                }
            },
            OptionArg::StringArray | OptionArg::FilenameArray => unsafe {
                // ponytail: write_string_array already overwrote the target with
                // `allocated_array`, so the original contents are gone; restoring
                // the previous length is the most this design can revert.
                if !change.allocated_array.is_empty() {
                    let target = &mut *(change.arg_data as *mut Vec<String>);
                    target.truncate(change.prev_array_len);
                }
            },
            OptionArg::Callback | OptionArg::Double | OptionArg::Int64 => {}
        }
    }
}

fn fail_parse(context: &mut OptionContext, error: Error) -> Error {
    revert_changes(context);
    context.pending_removals.clear();
    error
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn split_args(line: &str) -> Vec<String> {
        line.split_whitespace().map(str::to_owned).collect()
    }

    fn entry(
        long_name: &'static str,
        short_name: char,
        arg: OptionArg,
        arg_data: *mut c_void,
    ) -> OptionEntry {
        OptionEntry {
            long_name: Some(long_name),
            short_name,
            flags: option_flags::NONE,
            arg,
            arg_data,
            description: None,
            arg_description: None,
        }
    }

    #[test]
    fn option_context_new_and_setters() {
        let mut ctx = option_context_new(Some(""));
        assert!(ctx.get_help_enabled());
        assert!(!ctx.get_ignore_unknown_options());
        ctx.set_help_enabled(false);
        ctx.set_ignore_unknown_options(true);
        assert!(!ctx.get_help_enabled());
        assert!(ctx.get_ignore_unknown_options());
    }

    #[test]
    fn parse_int_last_value_wins() {
        let mut value = 0i32;
        let entries = [entry(
            "test",
            '\0',
            OptionArg::Int,
            &mut value as *mut i32 as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program --test 20 --test 30");
        ctx.parse(&mut argv).unwrap();
        assert_eq!(value, 30);
        assert_eq!(argv, vec!["program"]);
    }

    #[test]
    fn parse_string_last_value_wins() {
        let mut value = String::new();
        let entries = [entry(
            "test",
            '\0',
            OptionArg::String,
            &mut value as *mut String as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program --test foo --test bar");
        ctx.parse(&mut argv).unwrap();
        assert_eq!(value, "bar");
    }

    #[test]
    fn parse_bool_flag() {
        let mut flag = false;
        let entries = [entry(
            "test",
            '\0',
            OptionArg::None,
            &mut flag as *mut bool as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program --test");
        ctx.parse(&mut argv).unwrap();
        assert!(flag);
    }

    #[test]
    fn parse_string_array() {
        let mut values: Vec<String> = Vec::new();
        let entries = [entry(
            "test",
            '\0',
            OptionArg::StringArray,
            &mut values as *mut Vec<String> as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program --test foo --test bar");
        ctx.parse(&mut argv).unwrap();
        assert_eq!(values, vec!["foo", "bar"]);
    }

    #[test]
    fn unknown_option_error() {
        let entries = [OptionEntry::NULL];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program -0");
        let err = ctx.parse(&mut argv).unwrap_err();
        assert!(err.matches(option_error_quark(), OptionError::UnknownOption as i32));
    }

    #[test]
    fn ignore_unknown_long_option() {
        let mut flag = false;
        let entries = [entry(
            "test",
            '\0',
            OptionArg::None,
            &mut flag as *mut bool as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.set_ignore_unknown_options(true);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program --test --hello");
        ctx.parse(&mut argv).unwrap();
        assert!(flag);
        assert_eq!(argv, vec!["program", "--hello"]);
    }

    #[test]
    fn ignore_unknown_short_cluster() {
        let mut flag = false;
        let entries = [entry(
            "test",
            't',
            OptionArg::None,
            &mut flag as *mut bool as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.set_ignore_unknown_options(true);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program -test");
        ctx.parse(&mut argv).unwrap();
        assert!(flag);
        assert_eq!(argv, vec!["program", "-es"]);
    }

    #[test]
    fn missing_argument_error() {
        let mut value = String::new();
        let entries = [entry(
            "test",
            't',
            OptionArg::String,
            &mut value as *mut String as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program --test");
        let err = ctx.parse(&mut argv).unwrap_err();
        assert!(err.matches(option_error_quark(), OptionError::BadValue as i32));

        let mut argv = split_args("program -t");
        let err = ctx.parse(&mut argv).unwrap_err();
        assert!(err.matches(option_error_quark(), OptionError::BadValue as i32));
    }

    #[test]
    fn remaining_string_array() {
        let mut rest: Vec<String> = Vec::new();
        let entries = [OptionEntry {
            long_name: Some(OPTION_REMAINING),
            short_name: '\0',
            flags: option_flags::NONE,
            arg: OptionArg::StringArray,
            arg_data: &mut rest as *mut Vec<String> as *mut c_void,
            description: None,
            arg_description: None,
        }];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program foo bar");
        ctx.parse(&mut argv).unwrap();
        assert_eq!(rest, vec!["foo", "bar"]);
        assert_eq!(argv, vec!["program"]);
    }

    #[test]
    fn lonely_dash_is_non_option() {
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);

        let mut argv = split_args("program -");
        ctx.parse(&mut argv).unwrap();
        assert_eq!(argv, vec!["program", "-"]);
    }

    #[test]
    fn double_dash_separator() {
        let mut flag = false;
        let entries = [entry(
            "test",
            '\0',
            OptionArg::None,
            &mut flag as *mut bool as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program foo --test -- -bar");
        ctx.parse(&mut argv).unwrap();
        assert!(flag);
        assert_eq!(argv, vec!["program", "foo", "--", "-bar"]);
    }

    #[test]
    fn bad_int_restores_previous_value() {
        let mut value = 123i32;
        let entries = [entry(
            "test",
            '\0',
            OptionArg::Int,
            &mut value as *mut i32 as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program --test abc");
        let err = ctx.parse(&mut argv).unwrap_err();
        assert!(err.matches(option_error_quark(), OptionError::BadValue as i32));
        assert_eq!(value, 123);
    }

    #[test]
    fn long_option_equals_form() {
        let mut value = String::new();
        let entries = [entry(
            "test",
            '\0',
            OptionArg::String,
            &mut value as *mut String as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program --test=hello");
        ctx.parse(&mut argv).unwrap();
        assert_eq!(value, "hello");
    }

    fn callback_store(
        _option_name: &str,
        value: Option<&str>,
        data: *mut c_void,
    ) -> Result<(), Error> {
        unsafe {
            let slot = &*(data as *const RefCell<Option<String>>);
            slot.replace(value.map(str::to_owned));
        }
        Ok(())
    }

    #[test]
    fn callback_option() {
        let slot = RefCell::new(None);
        let entries = [OptionEntry {
            long_name: Some("test"),
            short_name: '\0',
            flags: option_flags::NONE,
            arg: OptionArg::Callback,
            arg_data: callback_store as *mut c_void,
            description: None,
            arg_description: None,
        }];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.main_group = Some(OptionGroup::new(
            None,
            None,
            None,
            &slot as *const RefCell<Option<String>> as *mut c_void,
        ));
        ctx.main_group.as_mut().unwrap().add_entries(&entries);

        let mut argv = split_args("program --test foo.txt");
        ctx.parse(&mut argv).unwrap();
        assert_eq!(slot.borrow().as_deref(), Some("foo.txt"));
    }

    #[test]
    fn option_group_add_entries() {
        let mut group = option_group_new(
            Some("test"),
            Some("Test Options"),
            Some("Show test options"),
            ptr::null_mut(),
        );
        let entries = [entry("switch", '\0', OptionArg::None, ptr::null_mut())];
        group.add_entries(&entries);
        assert_eq!(group.entry_count(), 1);
    }

    #[test]
    fn non_option_arguments_left_in_argv() {
        let mut flag = false;
        let entries = [entry(
            "test",
            '\0',
            OptionArg::None,
            &mut flag as *mut bool as *mut c_void,
        )];
        let mut ctx = option_context_new(None);
        ctx.set_help_enabled(false);
        ctx.add_main_entries(&entries, None);

        let mut argv = split_args("program foo --test bar");
        ctx.parse(&mut argv).unwrap();
        assert!(flag);
        assert_eq!(argv, vec!["program", "foo", "bar"]);
    }

    #[test]
    fn empty_argv_parse_succeeds() {
        let mut ctx = option_context_new(None);
        let mut argv: Vec<String> = Vec::new();
        ctx.parse(&mut argv).unwrap();
    }
}
