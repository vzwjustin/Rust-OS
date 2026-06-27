//! Phase 13 FFI parity tests.
//!
//! Validates that [`crate::ffi`] C entry points match expected GLib semantics
//! when invoked through `extern "C"` (as a C linker would).

#[cfg(test)]
mod tests {
    use crate::ffi::{gpointer, CGType, GError, GSignalCMarshaller, GValue};
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
}
