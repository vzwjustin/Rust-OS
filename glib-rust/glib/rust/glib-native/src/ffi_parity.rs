//! Phase 13 FFI parity tests.
//!
//! Validates that [`crate::ffi`] C entry points match expected GLib semantics
//! when invoked through `extern "C"` (as a C linker would).

#[cfg(test)]
mod tests {
    use crate::ffi::{
        gpointer, CGType, GBytesPtr, GClosurePtr, GError, GSignalCMarshaller, GValue,
    };
    use crate::gobject::object_new;
    use crate::gsignal::{signal_emit_by_name, signal_new, ConnectFlags, SignalFlags};
    use crate::gtype::{
        G_TYPE_BOOLEAN, G_TYPE_DOUBLE, G_TYPE_INVALID, G_TYPE_NONE, G_TYPE_OBJECT, G_TYPE_STRING,
        G_TYPE_UINT,
    };
    use crate::quark::quark_from_static_string;
    use crate::quark::Quark;
    use alloc::sync::Arc;
    use core::ffi::{c_char, c_void};
    use core::mem::MaybeUninit;
    use core::ptr;
    use core::sync::atomic::{AtomicI32, Ordering};
    use std::ffi::CString;

    extern "C" {
        fn g_malloc(n_bytes: usize) -> gpointer;
        fn g_malloc0(n_bytes: usize) -> gpointer;
        fn g_try_malloc(n_bytes: usize) -> gpointer;
        fn g_free(mem: gpointer);
        fn g_strdup(str: *const c_char) -> *mut c_char;
        fn g_strfreev(str_array: *mut *mut c_char);

        fn g_quark_from_static_string(string: *const c_char) -> Quark;
        fn g_quark_from_string(string: *const c_char) -> Quark;
        fn g_quark_to_string(quark: Quark) -> *const c_char;

        fn g_type_init();
        fn g_type_from_name(name: *const c_char) -> CGType;

        fn g_value_init(value: *mut GValue, type_: CGType);
        fn g_value_get_type(value: *const GValue) -> CGType;
        fn g_value_get_boolean(value: *const GValue) -> i32;
        fn g_value_set_boolean(value: *mut GValue, v: i32);
        fn g_value_get_string(value: *const GValue) -> *const c_char;
        fn g_value_set_string(value: *mut GValue, v: *const c_char);
        fn g_value_get_uint(value: *const GValue) -> u32;
        fn g_value_set_uint(value: *mut GValue, v: u32);
        fn g_value_get_double(value: *const GValue) -> f64;
        fn g_value_set_double(value: *mut GValue, v: f64);
        fn g_value_copy(src: *const GValue, dest: *mut GValue);
        fn g_value_reset(value: *mut GValue);

        fn g_object_ref(object: gpointer) -> gpointer;
        fn g_object_unref(object: gpointer);
        fn g_object_get_qdata(object: gpointer, quark: Quark) -> gpointer;
        fn g_object_set_qdata(object: gpointer, quark: Quark, data: gpointer);

        fn g_set_error(err: *mut *mut GError, domain: Quark, code: i32, message: *const c_char);
        fn g_clear_error(err: *mut *mut GError);
        fn g_propagate_error(dest: *mut *mut GError, src: *mut GError);

        fn g_signal_connect_data(
            instance: gpointer,
            detailed_signal: *const c_char,
            c_handler: GSignalCMarshaller,
            data: gpointer,
            destroy_data: Option<extern "C" fn(gpointer, gpointer)>,
            connect_flags: u32,
        ) -> u64;

        fn g_object_new(type_: CGType) -> gpointer;
        fn g_object_get_property(
            object: gpointer,
            property_name: *const c_char,
            value: *mut GValue,
        );
        fn g_object_set_property(
            object: gpointer,
            property_name: *const c_char,
            value: *const GValue,
        );
        fn g_object_is_floating(object: gpointer) -> i32;
        fn g_object_force_floating(object: gpointer);

        fn g_signal_new(
            signal_name: *const c_char,
            owner_type: CGType,
            flags: u32,
            return_type: CGType,
            n_params: u32,
            param_types: *const CGType,
        ) -> u32;
        fn g_signal_lookup(name: *const c_char, owner_type: CGType) -> u32;
        fn g_signal_name(signal_id: u32) -> *const c_char;
        fn g_signal_emit(
            instance: gpointer,
            signal_id: u32,
            detail: u32,
            args: *const GValue,
            n_args: u32,
        );
        fn g_signal_emit_by_name(
            instance: gpointer,
            detailed_signal: *const c_char,
            args: *const GValue,
            n_args: u32,
        );
        fn g_signal_handler_disconnect(handler_id: u64) -> i32;

        fn g_bytes_new(data: *const c_void, size: usize) -> GBytesPtr;
        fn g_bytes_new_take(data: *mut c_void, size: usize) -> GBytesPtr;
        fn g_bytes_get_data(bytes: GBytesPtr, size: *mut usize) -> *const c_void;
        fn g_bytes_get_size(bytes: GBytesPtr) -> usize;
        fn g_bytes_ref(bytes: GBytesPtr) -> GBytesPtr;
        fn g_bytes_unref(bytes: GBytesPtr);
        fn g_bytes_equal(bytes1: GBytesPtr, bytes2: GBytesPtr) -> i32;
        fn g_bytes_hash(bytes: GBytesPtr) -> u32;

        fn g_error_new(domain: Quark, code: i32, message: *const c_char) -> *mut GError;
        fn g_error_new_literal(domain: Quark, code: i32, message: *const c_char) -> *mut GError;
        fn g_error_copy(error: *const GError) -> *mut GError;
        fn g_error_matches(error: *const GError, domain: Quark, code: i32) -> i32;
        fn g_error_free(error: *mut GError);

        fn g_param_spec_boolean(
            name: *const c_char,
            nick: *const c_char,
            blurb: *const c_char,
            default_value: i32,
            flags: u32,
        ) -> gpointer;
        fn g_param_spec_string(
            name: *const c_char,
            nick: *const c_char,
            blurb: *const c_char,
            default_value: *const c_char,
            flags: u32,
        ) -> gpointer;
        fn g_param_spec_uint(
            name: *const c_char,
            nick: *const c_char,
            blurb: *const c_char,
            minimum: u32,
            maximum: u32,
            default_value: u32,
            flags: u32,
        ) -> gpointer;

        fn g_type_register_static(
            parent_type: CGType,
            type_name: *const c_char,
            info: *const crate::ffi::CGTypeInfo,
            flags: u32,
        ) -> CGType;
        fn g_type_register_static_simple(
            parent_type: CGType,
            type_name: *const c_char,
            class_size: u16,
            class_init: Option<extern "C" fn(*mut c_void)>,
            instance_size: u16,
            instance_init: Option<extern "C" fn(*mut c_void)>,
            flags: u32,
        ) -> CGType;

        fn g_strcmp0(str1: *const c_char, str2: *const c_char) -> i32;
        fn g_str_has_prefix(str: *const c_char, prefix: *const c_char) -> i32;
        fn g_str_has_suffix(str: *const c_char, suffix: *const c_char) -> i32;
        fn g_str_hash(str: *const c_char) -> u32;

        fn g_cclosure_new(
            callback: GSignalCMarshaller,
            data: gpointer,
            destroy_data: Option<extern "C" fn(gpointer, gpointer)>,
        ) -> GClosurePtr;
        fn g_closure_invoke(closure: GClosurePtr, args: *const GValue, n_args: u32);
        fn g_closure_ref(closure: GClosurePtr) -> GClosurePtr;
        fn g_closure_sink(closure: GClosurePtr);
        fn g_closure_unref(closure: GClosurePtr);
    }

    // ── Memory ───────────────────────────────────────────────────────────

    #[test]
    fn parity_malloc_write_read_free_roundtrip() {
        unsafe {
            let p = g_malloc(16);
            assert!(!p.is_null());
            ptr::write(p.cast::<u32>(), 0xdeadbeef);
            assert_eq!(ptr::read(p.cast::<u32>()), 0xdeadbeef);
            g_free(p);
        }
    }

    #[test]
    fn parity_malloc0_yields_zeroed_bytes() {
        unsafe {
            let p = g_malloc0(8);
            assert!(!p.is_null());
            let slice = core::slice::from_raw_parts(p.cast::<u8>(), 8);
            assert!(slice.iter().all(|&b| b == 0));
            g_free(p);
        }
    }

    #[test]
    fn parity_try_malloc_zero_size_returns_null() {
        unsafe {
            assert!(g_try_malloc(0).is_null());
        }
    }

    #[test]
    fn parity_strdup_and_strfreev() {
        let a = CString::new("alpha").unwrap();
        let b = CString::new("beta").unwrap();
        unsafe {
            let dup_a = g_strdup(a.as_ptr());
            let dup_b = g_strdup(b.as_ptr());
            assert!(!dup_a.is_null());
            assert!(!dup_b.is_null());

            let arr = g_malloc(3 * core::mem::size_of::<*mut c_char>()) as *mut *mut c_char;
            ptr::write(arr, dup_a);
            ptr::write(arr.add(1), dup_b);
            ptr::write(arr.add(2), ptr::null_mut());
            g_strfreev(arr);
        }
    }

    // ── Quark ────────────────────────────────────────────────────────────

    #[test]
    fn parity_quark_from_static_string() {
        static LABEL: &[u8] = b"parity-static-quark\0";
        unsafe {
            let q = g_quark_from_static_string(LABEL.as_ptr().cast());
            assert_ne!(q, 0);
            let roundtrip = g_quark_to_string(q);
            assert!(!roundtrip.is_null());
            assert_eq!(
                core::ffi::CStr::from_ptr(roundtrip).to_str().unwrap(),
                "parity-static-quark"
            );
        }
    }

    #[test]
    fn parity_quark_from_string_to_string() {
        let s = CString::new("parity-dynamic-quark").unwrap();
        unsafe {
            let q = g_quark_from_string(s.as_ptr());
            assert_ne!(q, 0);
            let c_str = g_quark_to_string(q);
            assert!(!c_str.is_null());
            assert_eq!(
                core::ffi::CStr::from_ptr(c_str).to_str().unwrap(),
                "parity-dynamic-quark"
            );
        }
    }

    // ── Type system ──────────────────────────────────────────────────────

    #[test]
    fn parity_type_init_is_idempotent() {
        unsafe {
            g_type_init();
            g_type_init();
            let name = CString::new("gboolean").unwrap();
            assert_eq!(g_type_from_name(name.as_ptr()), G_TYPE_BOOLEAN);
        }
    }

    #[test]
    fn parity_type_from_name_unknown_returns_invalid() {
        unsafe {
            g_type_init();
            let name = CString::new("NoSuchGTypeExistsInParityTests").unwrap();
            assert_eq!(g_type_from_name(name.as_ptr()), G_TYPE_INVALID);
        }
    }

    // ── GValue ───────────────────────────────────────────────────────────

    #[test]
    fn parity_value_boolean_init_get_set() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_BOOLEAN);
            assert_eq!(g_value_get_type(value.as_ptr()), G_TYPE_BOOLEAN);
            g_value_set_boolean(value.as_mut_ptr(), 1);
            assert_eq!(g_value_get_boolean(value.as_ptr()), 1);
        }
    }

    #[test]
    fn parity_value_string_init_get_set() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        let text = CString::new("parity-ffi-string").unwrap();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_STRING);
            assert_eq!(g_value_get_type(value.as_ptr()), G_TYPE_STRING);
            g_value_set_string(value.as_mut_ptr(), text.as_ptr());
            let out = g_value_get_string(value.as_ptr());
            assert!(!out.is_null());
            assert_eq!(
                core::ffi::CStr::from_ptr(out).to_str().unwrap(),
                "parity-ffi-string"
            );
        }
    }

    #[test]
    fn parity_value_uint_init_get_set() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_UINT);
            g_value_set_uint(value.as_mut_ptr(), 4242);
            assert_eq!(g_value_get_uint(value.as_ptr()), 4242);
        }
    }

    #[test]
    fn parity_value_double_init_get_set() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_DOUBLE);
            g_value_set_double(value.as_mut_ptr(), 1.2345);
            assert!((g_value_get_double(value.as_ptr()) - 1.2345).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn parity_value_copy() {
        let mut src: MaybeUninit<GValue> = MaybeUninit::uninit();
        let mut dest: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(src.as_mut_ptr(), G_TYPE_UINT);
            g_value_set_uint(src.as_mut_ptr(), 9001);
            g_value_init(dest.as_mut_ptr(), G_TYPE_UINT);
            g_value_copy(src.as_ptr(), dest.as_mut_ptr());
            assert_eq!(g_value_get_uint(dest.as_ptr()), 9001);
        }
    }

    #[test]
    fn parity_value_reset() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_UINT);
            g_value_set_uint(value.as_mut_ptr(), 55);
            g_value_reset(value.as_mut_ptr());
            assert_eq!(g_value_get_uint(value.as_ptr()), 0);
            assert_eq!(g_value_get_type(value.as_ptr()), G_TYPE_UINT);
        }
    }

    #[test]
    fn parity_value_get_type() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_DOUBLE);
            assert_eq!(g_value_get_type(value.as_ptr()), G_TYPE_DOUBLE);
        }
    }

    // ── GObject ──────────────────────────────────────────────────────────

    #[test]
    fn parity_object_ref_unref_refcount() {
        unsafe {
            g_type_init();
        }
        let obj: Arc<crate::gobject::GObject> = object_new(G_TYPE_OBJECT);
        assert_eq!(obj.ref_count(), 1);
        let raw = Arc::into_raw(obj) as gpointer;
        unsafe {
            g_object_ref(raw);
            assert_eq!((&*raw.cast::<crate::gobject::GObject>()).ref_count(), 2);
            g_object_unref(raw);
            assert_eq!((&*raw.cast::<crate::gobject::GObject>()).ref_count(), 1);
            g_object_unref(raw);
        }
    }

    #[test]
    fn parity_object_qdata_set_get() {
        unsafe {
            g_type_init();
        }
        let obj: Arc<crate::gobject::GObject> = object_new(G_TYPE_OBJECT);
        let raw = Arc::into_raw(obj) as gpointer;
        let q = quark_from_static_string(Some("parity-qdata-key"));
        let payload = 0xfeed_usize as *mut c_void;
        unsafe {
            g_object_set_qdata(raw, q, payload);
            assert_eq!(g_object_get_qdata(raw, q), payload);
            g_object_unref(raw);
        }
    }

    // ── GError ───────────────────────────────────────────────────────────

    #[test]
    fn parity_set_and_clear_error() {
        let domain = quark_from_static_string(Some("parity-error-domain"));
        let mut err: *mut GError = ptr::null_mut();
        let msg = CString::new("parity failure").unwrap();
        unsafe {
            g_set_error(&mut err, domain, 42, msg.as_ptr());
            assert!(!err.is_null());
            assert_eq!((*err).domain, domain);
            assert_eq!((*err).code, 42);
            assert_eq!(
                core::ffi::CStr::from_ptr((*err).message).to_str().unwrap(),
                "parity failure"
            );
            g_clear_error(&mut err);
            assert!(err.is_null());
        }
    }

    #[test]
    fn parity_propagate_error() {
        let domain = quark_from_static_string(Some("parity-prop-domain"));
        let mut src: *mut GError = ptr::null_mut();
        let mut dest: *mut GError = ptr::null_mut();
        let msg = CString::new("propagated parity").unwrap();
        unsafe {
            g_set_error(&mut src, domain, 11, msg.as_ptr());
            g_propagate_error(&mut dest, src);
            assert!(!dest.is_null());
            assert_eq!((*dest).code, 11);
            assert_eq!(
                core::ffi::CStr::from_ptr((*dest).message).to_str().unwrap(),
                "propagated parity"
            );
            g_clear_error(&mut dest);
        }
    }

    // ── Signals ──────────────────────────────────────────────────────────

    #[test]
    fn parity_signal_connect_and_emit() {
        static HITS: AtomicI32 = AtomicI32::new(0);
        extern "C" fn on_parity_signal(_inst: gpointer, _data: gpointer) {
            HITS.fetch_add(1, Ordering::SeqCst);
        }

        unsafe {
            g_type_init();
        }
        signal_new(
            "parity-ffi-signal",
            G_TYPE_OBJECT,
            SignalFlags::RUN_LAST,
            G_TYPE_NONE,
            &[],
        );
        let obj: Arc<crate::gobject::GObject> = object_new(G_TYPE_OBJECT);
        let raw = Arc::into_raw(obj) as gpointer;
        let sig = CString::new("parity-ffi-signal").unwrap();
        unsafe {
            let handler_id = g_signal_connect_data(
                raw,
                sig.as_ptr(),
                Some(on_parity_signal),
                ptr::null_mut(),
                None,
                ConnectFlags::NONE.0,
            );
            assert!(
                handler_id > 0,
                "g_signal_connect_data must return non-zero id"
            );
            HITS.store(0, Ordering::SeqCst);
            signal_emit_by_name(G_TYPE_OBJECT, "parity-ffi-signal", &[]);
            assert_eq!(
                HITS.load(Ordering::SeqCst),
                1,
                "connected handler must run exactly once on emit"
            );
            g_object_unref(raw);
        }
    }

    // ── GObject: construction + properties ──────────────────────────────

    #[test]
    fn parity_object_new_via_ffi() {
        unsafe {
            g_type_init();
            let obj = g_object_new(G_TYPE_OBJECT);
            assert!(!obj.is_null());
            let rc = (&*obj.cast::<crate::gobject::GObject>()).ref_count();
            assert_eq!(rc, 1);
            g_object_unref(obj);
        }
    }

    #[test]
    fn parity_object_is_floating_and_force_floating() {
        unsafe {
            g_type_init();
            let obj = g_object_new(G_TYPE_OBJECT);
            assert_eq!(g_object_is_floating(obj), 0);
            g_object_force_floating(obj);
            assert_eq!(g_object_is_floating(obj), 1);
            g_object_unref(obj);
        }
    }

    // ── GSignal: new / lookup / name / emit / disconnect ────────────────

    #[test]
    fn parity_signal_new_lookup_name() {
        let name = CString::new("parity-ffi-signal-new").unwrap();
        unsafe {
            g_type_init();
            let sid = g_signal_new(
                name.as_ptr(),
                G_TYPE_OBJECT,
                SignalFlags::RUN_LAST.0,
                G_TYPE_NONE,
                0,
                ptr::null(),
            );
            assert!(sid > 0);
            let sid2 = g_signal_lookup(name.as_ptr(), G_TYPE_OBJECT);
            assert_eq!(sid, sid2);
            let name_ptr = g_signal_name(sid);
            assert!(!name_ptr.is_null());
            assert_eq!(
                core::ffi::CStr::from_ptr(name_ptr).to_str().unwrap(),
                "parity-ffi-signal-new"
            );
        }
    }

    #[test]
    fn parity_signal_emit_by_name_ffi() {
        static FFI_HITS: AtomicI32 = AtomicI32::new(0);
        extern "C" fn on_ffi_emit(_inst: gpointer, _data: gpointer) {
            FFI_HITS.fetch_add(1, Ordering::SeqCst);
        }

        let sig_name = CString::new("parity-ffi-emit-by-name").unwrap();
        unsafe {
            g_type_init();
            g_signal_new(
                sig_name.as_ptr(),
                G_TYPE_OBJECT,
                SignalFlags::RUN_LAST.0,
                G_TYPE_NONE,
                0,
                ptr::null(),
            );
            let obj = g_object_new(G_TYPE_OBJECT);
            let handler_id = g_signal_connect_data(
                obj,
                sig_name.as_ptr(),
                Some(on_ffi_emit),
                ptr::null_mut(),
                None,
                ConnectFlags::NONE.0,
            );
            assert!(handler_id > 0);
            FFI_HITS.store(0, Ordering::SeqCst);
            g_signal_emit_by_name(obj, sig_name.as_ptr(), ptr::null(), 0);
            assert_eq!(FFI_HITS.load(Ordering::SeqCst), 1);
            let rc = g_signal_handler_disconnect(handler_id);
            assert_eq!(rc, 1);
            FFI_HITS.store(0, Ordering::SeqCst);
            g_signal_emit_by_name(obj, sig_name.as_ptr(), ptr::null(), 0);
            assert_eq!(FFI_HITS.load(Ordering::SeqCst), 0);
            g_object_unref(obj);
        }
    }

    // ── GBytes ──────────────────────────────────────────────────────────

    #[test]
    fn parity_bytes_new_get_data_get_size() {
        let data = b"hello bytes";
        unsafe {
            let bytes = g_bytes_new(data.as_ptr() as *const c_void, data.len());
            assert!(!bytes.is_null());
            let mut size: usize = 0;
            let ptr = g_bytes_get_data(bytes, &mut size);
            assert_eq!(size, data.len());
            assert!(!ptr.is_null());
            let slice = core::slice::from_raw_parts(ptr.cast::<u8>(), size);
            assert_eq!(slice, data);
            assert_eq!(g_bytes_get_size(bytes), data.len());
            g_bytes_unref(bytes);
        }
    }

    #[test]
    fn parity_bytes_ref_unref() {
        let data = b"refcount";
        unsafe {
            let bytes = g_bytes_new(data.as_ptr() as *const c_void, data.len());
            let bytes2 = g_bytes_ref(bytes);
            assert!(!bytes2.is_null());
            g_bytes_unref(bytes2);
            g_bytes_unref(bytes);
        }
    }

    #[test]
    fn parity_bytes_equal_and_hash() {
        let data = b"equal bytes";
        unsafe {
            let b1 = g_bytes_new(data.as_ptr() as *const c_void, data.len());
            let b2 = g_bytes_new(data.as_ptr() as *const c_void, data.len());
            assert_eq!(g_bytes_equal(b1, b2), 1);
            let h1 = g_bytes_hash(b1);
            let h2 = g_bytes_hash(b2);
            assert_eq!(h1, h2);
            g_bytes_unref(b1);
            g_bytes_unref(b2);
        }
    }

    #[test]
    fn parity_bytes_new_take() {
        let data = b"take ownership";
        unsafe {
            let buf = g_malloc(data.len());
            ptr::copy_nonoverlapping(data.as_ptr(), buf.cast::<u8>(), data.len());
            let bytes = g_bytes_new_take(buf, data.len());
            assert!(!bytes.is_null());
            assert_eq!(g_bytes_get_size(bytes), data.len());
            g_bytes_unref(bytes);
        }
    }

    // ── GError: new / copy / matches ────────────────────────────────────

    #[test]
    fn parity_error_new_and_matches() {
        let domain = quark_from_static_string(Some("parity-err-domain"));
        let msg = CString::new("ffi error test").unwrap();
        unsafe {
            let err = g_error_new(domain, 99, msg.as_ptr());
            assert!(!err.is_null());
            assert_eq!((*err).domain, domain);
            assert_eq!((*err).code, 99);
            assert_eq!(
                core::ffi::CStr::from_ptr((*err).message).to_str().unwrap(),
                "ffi error test"
            );
            assert_eq!(g_error_matches(err, domain, 99), 1);
            assert_eq!(g_error_matches(err, domain, 0), 0);
            g_error_free(err);
        }
    }

    #[test]
    fn parity_error_copy() {
        let domain = quark_from_static_string(Some("parity-copy-domain"));
        let msg = CString::new("copy me").unwrap();
        unsafe {
            let err = g_error_new_literal(domain, 7, msg.as_ptr());
            let copy = g_error_copy(err);
            assert!(!copy.is_null());
            assert_eq!((*copy).domain, domain);
            assert_eq!((*copy).code, 7);
            assert_eq!(
                core::ffi::CStr::from_ptr((*copy).message).to_str().unwrap(),
                "copy me"
            );
            g_error_free(err);
            g_error_free(copy);
        }
    }

    // ── GParamSpec: boolean / string / uint ────────────────────────────

    #[test]
    fn parity_param_spec_boolean() {
        let name = CString::new("active").unwrap();
        let nick = CString::new("Active").unwrap();
        let blurb = CString::new("Whether active").unwrap();
        unsafe {
            let spec = g_param_spec_boolean(name.as_ptr(), nick.as_ptr(), blurb.as_ptr(), 1, 0);
            assert!(!spec.is_null());
        }
    }

    #[test]
    fn parity_param_spec_string() {
        let name = CString::new("name").unwrap();
        let nick = CString::new("Name").unwrap();
        let blurb = CString::new("Name property").unwrap();
        let default_val = CString::new("default").unwrap();
        unsafe {
            let spec = g_param_spec_string(
                name.as_ptr(),
                nick.as_ptr(),
                blurb.as_ptr(),
                default_val.as_ptr(),
                0,
            );
            assert!(!spec.is_null());
        }
    }

    #[test]
    fn parity_param_spec_uint() {
        let name = CString::new("count").unwrap();
        let nick = CString::new("Count").unwrap();
        let blurb = CString::new("Count property").unwrap();
        unsafe {
            let spec =
                g_param_spec_uint(name.as_ptr(), nick.as_ptr(), blurb.as_ptr(), 0, 100, 50, 0);
            assert!(!spec.is_null());
        }
    }

    // ── GType: registration ────────────────────────────────────────────

    #[test]
    fn parity_type_register_static_simple() {
        let type_name = CString::new("ParityTestType").unwrap();
        unsafe {
            g_type_init();
            let type_id = g_type_register_static_simple(
                G_TYPE_OBJECT,
                type_name.as_ptr(),
                0,
                None,
                0,
                None,
                0,
            );
            assert_ne!(type_id, G_TYPE_INVALID);
            // Registering the same name again should fail
            let dup = g_type_register_static_simple(
                G_TYPE_OBJECT,
                type_name.as_ptr(),
                0,
                None,
                0,
                None,
                0,
            );
            assert_eq!(dup, G_TYPE_INVALID);
        }
    }

    // ── String helpers ─────────────────────────────────────────────────

    #[test]
    fn parity_strcmp0() {
        let a = CString::new("alpha").unwrap();
        let b = CString::new("beta").unwrap();
        let a2 = CString::new("alpha").unwrap();
        unsafe {
            assert_eq!(g_strcmp0(a.as_ptr(), a2.as_ptr()), 0);
            assert!(g_strcmp0(a.as_ptr(), b.as_ptr()) < 0);
            assert!(g_strcmp0(b.as_ptr(), a.as_ptr()) > 0);
            assert_eq!(g_strcmp0(ptr::null(), ptr::null()), 0);
            assert!(g_strcmp0(ptr::null(), a.as_ptr()) < 0);
            assert!(g_strcmp0(a.as_ptr(), ptr::null()) > 0);
        }
    }

    #[test]
    fn parity_str_has_prefix_suffix() {
        let s = CString::new("hello world").unwrap();
        let p = CString::new("hello").unwrap();
        let sf = CString::new("world").unwrap();
        unsafe {
            assert_eq!(g_str_has_prefix(s.as_ptr(), p.as_ptr()), 1);
            assert_eq!(g_str_has_suffix(s.as_ptr(), sf.as_ptr()), 1);
            assert_eq!(g_str_has_prefix(s.as_ptr(), sf.as_ptr()), 0);
        }
    }

    #[test]
    fn parity_str_hash_consistent() {
        let s1 = CString::new("hashme").unwrap();
        let s2 = CString::new("hashme").unwrap();
        unsafe {
            assert_eq!(g_str_hash(s1.as_ptr()), g_str_hash(s2.as_ptr()));
            assert_ne!(g_str_hash(s1.as_ptr()), 0);
        }
    }

    // ── GClosure ───────────────────────────────────────────────────────

    #[test]
    fn parity_cclosure_new_and_invoke() {
        static CLOSURE_HITS: AtomicI32 = AtomicI32::new(0);
        extern "C" fn on_closure(_data: gpointer, _unused: gpointer) {
            CLOSURE_HITS.fetch_add(1, Ordering::SeqCst);
        }
        unsafe {
            CLOSURE_HITS.store(0, Ordering::SeqCst);
            let closure = g_cclosure_new(Some(on_closure), ptr::null_mut(), None);
            assert!(!closure.is_null());
            g_closure_sink(closure);
            g_closure_invoke(closure, ptr::null(), 0);
            assert_eq!(CLOSURE_HITS.load(Ordering::SeqCst), 1);
            g_closure_unref(closure);
        }
    }
}
