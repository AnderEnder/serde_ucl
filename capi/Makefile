# A separate Cargo package: the Rust crate does not build or install the C ABI by default.
PREFIX ?= /usr/local
DESTDIR ?=
CARGO ?= cargo
UNAME := $(shell uname -s)
ifeq ($(UNAME),Darwin)
SHARED = libucl.dylib
else
SHARED = libucl.so
endif

.PHONY: all install check clean
all:
	$(CARGO) build --release --manifest-path Cargo.toml

install: all
	install -d "$(DESTDIR)$(PREFIX)/include" "$(DESTDIR)$(PREFIX)/lib"
	install -m 644 include/ucl.h "$(DESTDIR)$(PREFIX)/include/ucl.h"
	install -m 644 target/release/libucl.a "$(DESTDIR)$(PREFIX)/lib/libucl.a"
	install -m 755 target/release/$(SHARED) "$(DESTDIR)$(PREFIX)/lib/$(SHARED)"
ifeq ($(UNAME),Darwin)
	install_name_tool -id @rpath/libucl.dylib "$(DESTDIR)$(PREFIX)/lib/libucl.dylib"
endif

check:
	python3 tests/check.py

clean:
	$(CARGO) clean --manifest-path Cargo.toml
