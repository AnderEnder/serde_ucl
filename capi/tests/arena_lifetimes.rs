//! Run this ownership/provenance regression with `cargo +nightly miri test
//! --manifest-path capi/Cargo.toml --test arena_lifetimes` and
//! `MIRIFLAGS=-Zmiri-disable-isolation` for the creation-time cwd service.
use std::ffi::CStr;

#[test]
fn retained_arena_and_forced_buffers_keep_valid_provenance() {
    unsafe {
        let parser = ucl::ucl_parser_new(0);
        assert!(ucl::ucl_parser_add_string(
            parser,
            c"child={s='hello';x=42;y=3.5;ar=[1,2]};other={n=9};dup=1;dup=2".as_ptr(),
            0,
        ));
        let root = ucl::ucl_parser_get_object(parser);
        let child = ucl::ucl_object_ref(ucl::ucl_object_lookup(root, c"child".as_ptr()));
        let other = ucl::ucl_object_ref(ucl::ucl_object_lookup(root, c"other".as_ptr()));
        let text = ucl::ucl_object_tostring(ucl::ucl_object_lookup(child, c"s".as_ptr()));
        let number = ucl::ucl_object_lookup(child, c"x".as_ptr());
        let forced = ucl::ucl_object_tostring_forced(number);
        let duplicate = ucl::ucl_object_lookup(root, c"dup".as_ptr());
        assert_eq!(ucl::ucl_object_toint(duplicate), 1);
        assert_eq!(ucl::ucl_object_toint((*duplicate).next), 2);
        ucl::ucl_parser_free(parser);
        ucl::ucl_object_unref(root);
        assert_eq!((*child).r#ref, 1);
        assert_eq!((*other).r#ref, 1);
        ucl::ucl_object_unref(other);
        assert_eq!(CStr::from_ptr(text).to_bytes(), b"hello");
        assert_eq!(CStr::from_ptr(forced).to_bytes(), b"42");
        let iterator = ucl::ucl_object_iterate_new(child);
        assert_eq!(
            ucl::ucl_object_iterate_safe(iterator, true),
            ucl::ucl_object_lookup(child, c"s".as_ptr())
        );
        ucl::ucl_object_iterate_reset(iterator, child);
        let mut count = 0;
        while !ucl::ucl_object_iterate_full(iterator, 3).is_null() {
            count += 1;
        }
        assert_eq!(count, 4);
        ucl::ucl_object_iterate_free(iterator);
        // Grow the arena-owned cache beyond its first vector allocation. Inner bytes
        // must retain their addresses while cache metadata moves.
        for key in [c"y", c"ar"] {
            assert!(
                !ucl::ucl_object_tostring_forced(ucl::ucl_object_lookup(child, key.as_ptr()))
                    .is_null()
            );
        }
        let array = ucl::ucl_object_lookup(child, c"ar".as_ptr());
        for index in 0..2 {
            assert!(
                !ucl::ucl_object_tostring_forced(ucl::ucl_array_find_index(array, index)).is_null()
            );
        }
        assert_eq!(CStr::from_ptr(forced).to_bytes(), b"42");
        assert_eq!(ucl::ucl_object_tostring_forced(number), forced);
        let mut length = 0;
        let emitted = ucl::ucl_object_emit_len(child, 2, &mut length);
        assert!(!emitted.is_null());
        let bytes = std::slice::from_raw_parts(emitted, length);
        assert!(bytes.windows(7).any(|bytes| bytes == b"'hello'"));
        libc::free(emitted.cast());
        ucl::ucl_object_unref(child);
    }
}

#[test]
fn duplicate_heads_and_nested_arrays_survive_parent_release() {
    unsafe {
        let parser = ucl::ucl_parser_new(0);
        assert!(ucl::ucl_parser_add_string(
            parser,
            c"container={dup=1;dup={deep='yes'};tail=9;array=[1,{n=2},[3]]};unused=0".as_ptr(),
            0,
        ));
        let root = ucl::ucl_parser_get_object(parser);
        let container = ucl::ucl_object_ref(ucl::ucl_object_lookup(root, c"container".as_ptr()));
        let duplicate = ucl::ucl_object_lookup(container, c"dup".as_ptr());
        let second = (*duplicate).next;
        let array = ucl::ucl_object_lookup(container, c"array".as_ptr());
        let nested = ucl::ucl_array_find_index(array, 1);
        ucl::ucl_parser_free(parser);
        ucl::ucl_object_unref(root);
        assert_eq!((*container).r#ref, 1);
        assert_eq!((*duplicate).r#ref, 1);
        assert_eq!((*second).r#ref, 1);
        assert_eq!((*array).r#ref, 1);
        assert_eq!((*nested).r#ref, 1);
        assert_eq!(ucl::ucl_object_toint(duplicate), 1);
        assert_eq!(
            CStr::from_ptr(ucl::ucl_object_tostring(ucl::ucl_object_lookup(
                second,
                c"deep".as_ptr()
            )))
            .to_bytes(),
            b"yes",
        );
        assert_eq!(
            ucl::ucl_object_toint(ucl::ucl_object_lookup(nested, c"n".as_ptr())),
            2
        );
        let iterator = ucl::ucl_object_iterate_new(container);
        for expected in [
            duplicate,
            ucl::ucl_object_lookup(container, c"tail".as_ptr()),
            array,
        ] {
            assert_eq!(ucl::ucl_object_iterate_safe(iterator, true), expected);
        }
        assert!(ucl::ucl_object_iterate_safe(iterator, true).is_null());
        ucl::ucl_object_iterate_reset(iterator, array);
        for index in 0..3 {
            assert_eq!(
                ucl::ucl_object_iterate_safe(iterator, true),
                ucl::ucl_array_find_index(array, index)
            );
        }
        assert!(ucl::ucl_object_iterate_safe(iterator, true).is_null());
        ucl::ucl_object_iterate_free(iterator);
        ucl::ucl_object_unref(container);
    }
}

#[test]
fn default_submission_owns_input_and_retained_scalar_emission_facts() {
    unsafe {
        let mut input =
            b"single='first';plain=\"normal\";later='last';block=<<EOD\nline1\nline2\nEOD\n"
                .to_vec();
        let parser = ucl::ucl_parser_new(0);
        assert!(ucl::ucl_parser_add_chunk(
            parser,
            input.as_ptr(),
            input.len()
        ));
        input.fill(0);
        drop(input);
        let root = ucl::ucl_parser_get_object(parser);
        let single = ucl::ucl_object_ref(ucl::ucl_object_lookup(root, c"single".as_ptr()));
        let plain = ucl::ucl_object_ref(ucl::ucl_object_lookup(root, c"plain".as_ptr()));
        let block = ucl::ucl_object_ref(ucl::ucl_object_lookup(root, c"block".as_ptr()));
        ucl::ucl_parser_free(parser);
        ucl::ucl_object_unref(root);
        for (scalar, expected) in [
            (single, b"'first'".as_slice()),
            (plain, b"\"normal\"".as_slice()),
            (block, b"<<EOD\nline1\nline2\nEOD".as_slice()),
        ] {
            assert_eq!((*scalar).r#ref, 1);
            let mut length = 0;
            let emitted = ucl::ucl_object_emit_len(scalar, 2, &mut length);
            assert!(!emitted.is_null());
            assert_eq!(std::slice::from_raw_parts(emitted, length), expected);
            libc::free(emitted.cast());
            ucl::ucl_object_unref(scalar);
        }
    }
}
