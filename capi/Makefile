# Opt-in C install using exact reviewed declarations and ABI-versioned libraries.
PREFIX ?= /usr/local
LIBDIR ?= $(PREFIX)/lib
DESTDIR ?=
CARGO ?= cargo

.PHONY: all install check clean
all:
	$(CARGO) build --locked --release --manifest-path Cargo.toml

install:
	$(CARGO) cinstall --locked --release --manifest-path Cargo.toml --prefix "$(PREFIX)" --libdir "$(LIBDIR)" $(if $(DESTDIR),--destdir "$(DESTDIR)")

check:
	python3 tests/check.py

clean:
	$(CARGO) clean --manifest-path Cargo.toml
