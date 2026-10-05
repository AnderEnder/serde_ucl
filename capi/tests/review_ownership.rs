//! Independent C15 review: memory-input ownership versus the ordinary file path.
use std::ffi::{CStr, CString};

unsafe fn emission(node: *const ucl::UclObject, format: i32) -> Vec<u8> {
    let mut len = 0;
    let buffer = unsafe { ucl::ucl_object_emit_len(node, format, &mut len) };
    assert!(!buffer.is_null());
    let result = unsafe { std::slice::from_raw_parts(buffer, len) }.to_vec();
    assert_eq!(unsafe { *buffer.add(len) }, 0);
    unsafe { libc::free(buffer.cast()) };
    result
}

unsafe fn compare(left: *const ucl::UclObject, right: *const ucl::UclObject) {
    let mut pending = vec![(left, right)];
    while let Some((left, right)) = pending.pop() {
        assert_eq!(unsafe { (*left).r#type }, unsafe { (*right).r#type });
        assert_eq!(unsafe { (*left).len }, unsafe { (*right).len });
        assert_eq!(unsafe { (*left).flags }, unsafe { (*right).flags });
        for format in 0..4 {
            assert_eq!(unsafe { emission(left, format) }, unsafe {
                emission(right, format)
            });
        }
        if unsafe { (*left).r#type } < 2 {
            let li = unsafe { ucl::ucl_object_iterate_new(left) };
            let ri = unsafe { ucl::ucl_object_iterate_new(right) };
            loop {
                let l = unsafe { ucl::ucl_object_iterate_safe(li, true) };
                let r = unsafe { ucl::ucl_object_iterate_safe(ri, true) };
                assert_eq!(l.is_null(), r.is_null());
                if l.is_null() {
                    break;
                }
                pending.push((l, r));
            }
            unsafe {
                ucl::ucl_object_iterate_free(li);
                ucl::ucl_object_iterate_free(ri);
            }
        }
        let l = unsafe { (*left).next };
        let r = unsafe { (*right).next };
        assert_eq!(l.is_null(), r.is_null());
        if !l.is_null() {
            pending.push((l, r));
        }
    }
}

#[test]
fn retained_mixed_sources_match_owned_file_path_after_memory_is_destroyed() {
    let directory = std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!("c15-review-ownership-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let include = directory.join("included.ucl");
    let loaded = directory.join("loaded.txt");
    let main = directory.join("main.ucl");
    std::fs::write(&include, "inc={one='included';two=\"$REVIEW\"};\n").unwrap();
    std::fs::write(&loaded, "loaded first\nloaded second\n").unwrap();
    let document = format!(
        "keep={{plain=\"first\";single='one';last='two';escaped=\"a\\nb\";\
         block=<<END\nfirst\nsecond\nEND\n\
         variable=\"$REVIEW\";nul=\"a\\u0000b\";array=[1,2s,true,{{deep='nested'}}];\
         dup=1;dup={{value='duplicate'}};\n.include \"{}\"\n\
         .load(key=\"loaded\";multiline=true) \"{}\"\n}};unused=99",
        include.display(),
        loaded.display()
    );
    std::fs::write(&main, &document).unwrap();
    unsafe {
        for flags in [0, serde_ucl::value::ParserFlags::ZEROCOPY.bits() as i32] {
            let memory = ucl::ucl_parser_new(flags);
            let file = ucl::ucl_parser_new(flags);
            let variable = CString::new("registered independent text").unwrap();
            for parser in [memory, file] {
                ucl::ucl_parser_register_variable(parser, c"REVIEW".as_ptr(), variable.as_ptr());
            }
            drop(variable);
            let mut input = document.as_bytes().to_vec();
            assert!(ucl::ucl_parser_add_chunk(
                memory,
                input.as_ptr(),
                input.len()
            ));
            // Default callers may immediately destroy their storage. ZEROCOPY
            // callers keep it unchanged until every derived object is released.
            if flags == 0 {
                input.fill(0xaa);
                input.clear();
                input.shrink_to_fit();
            }
            let name = CString::new(main.to_str().unwrap()).unwrap();
            assert!(ucl::ucl_parser_add_file(file, name.as_ptr()));
            let mr = ucl::ucl_parser_get_object(memory);
            let fr = ucl::ucl_parser_get_object(file);
            let mc = ucl::ucl_object_ref(ucl::ucl_object_lookup(mr, c"keep".as_ptr()));
            let fc = ucl::ucl_object_ref(ucl::ucl_object_lookup(fr, c"keep".as_ptr()));
            ucl::ucl_parser_free(memory);
            ucl::ucl_parser_free(file);
            ucl::ucl_object_unref(mr);
            ucl::ucl_object_unref(fr);
            assert_eq!((*mc).r#ref, 1);
            assert_eq!(
                CStr::from_ptr(ucl::ucl_object_tostring(ucl::ucl_object_lookup(
                    mc,
                    c"variable".as_ptr()
                )))
                .to_bytes(),
                b"registered independent text"
            );
            compare(mc, fc);
            let ms = ucl::ucl_object_ref(ucl::ucl_object_lookup(mc, c"single".as_ptr()));
            let fs = ucl::ucl_object_ref(ucl::ucl_object_lookup(fc, c"single".as_ptr()));
            ucl::ucl_object_unref(mc);
            ucl::ucl_object_unref(fc);
            compare(ms, fs);
            assert_eq!(
                CStr::from_ptr(ucl::ucl_object_tostring(ms)).to_bytes(),
                b"one"
            );
            ucl::ucl_object_unref(ms);
            ucl::ucl_object_unref(fs);
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn retained_partial_graph_does_not_borrow_destroyed_caller_input() {
    unsafe {
        let parser = ucl::ucl_parser_new(0);
        let mut input = b"keep={text='partial';array=[1,2]};unfinished={".to_vec();
        assert!(!ucl::ucl_parser_add_chunk(
            parser,
            input.as_ptr(),
            input.len()
        ));
        input.fill(0);
        drop(input);
        let root = ucl::ucl_parser_get_object(parser);
        assert!(!root.is_null());
        let keep = ucl::ucl_object_ref(ucl::ucl_object_lookup(root, c"keep".as_ptr()));
        ucl::ucl_parser_free(parser);
        ucl::ucl_object_unref(root);
        assert_eq!((*keep).r#ref, 1);
        assert_eq!(emission(keep, 1), b"{\"text\":\"partial\",\"array\":[1,2]}");
        for format in 0..4 {
            assert!(!emission(keep, format).is_empty());
        }
        ucl::ucl_object_unref(keep);
    }
}

#[test]
fn scalar_fact_compaction_matches_each_original_rust_cursor() {
    use serde_ucl::emit::{Emitter, Format};
    use serde_ucl::parse::{Parser, PathSegment};

    let document = b"plain=\"first\";single='one';later='two';escaped=\"a\\nb\";\
        block=<<END\nfirst\nsecond\nEND\n\
        other=<<END\nthird\nfourth\nEND\n\
        integer=42;boolean=true;time=2s;float=3.5;nil=null;\
        container={single='inner';array=[\"plain\",'quoted',4]};\
        \"quoted key\"='key facts'";
    let mut parser = Parser::new();
    let rust = parser.parse(document).unwrap();
    unsafe {
        let cparser = ucl::ucl_parser_new(0);
        assert!(ucl::ucl_parser_add_chunk(
            cparser,
            document.as_ptr(),
            document.len()
        ));
        let root = ucl::ucl_parser_get_object(cparser);
        ucl::ucl_parser_free(cparser);
        for (key, value) in rust.as_object().unwrap().iter() {
            let name = CString::new(key.as_bytes()).unwrap();
            let cvalue = ucl::ucl_object_ref(ucl::ucl_object_lookup(root, name.as_ptr()));
            let path = [PathSegment::Key {
                key: key.to_string(),
                index: 0,
            }];
            for (selector, format) in [
                Format::Json,
                Format::JsonCompact,
                Format::Config,
                Format::Yaml,
            ]
            .into_iter()
            .enumerate()
            {
                let expected = Emitter::new(format)
                    .with_facts_path(parser.output_facts(), &path)
                    .emit(value.slots()[0].value());
                assert_eq!(
                    emission(cvalue, selector as i32),
                    expected.as_bytes(),
                    "{key}"
                );
            }
            ucl::ucl_object_unref(cvalue);
        }
        ucl::ucl_object_unref(root);
    }
}
