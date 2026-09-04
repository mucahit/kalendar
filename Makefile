PREFIX ?= /usr/local
CARGO ?= cargo

.PHONY: build test lint install uninstall

build:
	$(CARGO) build --release

test:
	$(CARGO) test --workspace

lint:
	$(CARGO) fmt --all -- --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings

install: build
	install -d "$(DESTDIR)$(PREFIX)/bin" "$(DESTDIR)$(PREFIX)/libexec/kalendar"
	install -m 755 target/release/kalendar "$(DESTDIR)$(PREFIX)/bin/kalendar"
	install -m 755 $$(find target/release/build -path '*/out/kalendar-eventkit' -type f | head -n 1) "$(DESTDIR)$(PREFIX)/libexec/kalendar/kalendar-eventkit"

uninstall:
	rm -f "$(DESTDIR)$(PREFIX)/bin/kalendar"
	rm -f "$(DESTDIR)$(PREFIX)/libexec/kalendar/kalendar-eventkit"
	rmdir "$(DESTDIR)$(PREFIX)/libexec/kalendar" 2>/dev/null || true

