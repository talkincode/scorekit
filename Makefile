SHELL := /bin/sh

CARGO ?= cargo
MDBOOK ?= mdbook
INSTALL ?= install
COPY ?= cp
PREFIX ?= $(HOME)/.local
BINDIR ?= $(PREFIX)/bin
SKILLS_DIR ?= $(HOME)/.agents/skills
TARGET_DIR ?= target
SCOREKIT_BIN := $(TARGET_DIR)/release/scorekit
SFIZZ_BIN ?= assets/bin/sfizz_render
SCOREKIT_SOUND_LIBRARY_DIR ?= $(PREFIX)/share/scorekit/sounds
SCOREKIT_DEFAULT_SOUNDFONT := $(SCOREKIT_SOUND_LIBRARY_DIR)/sf2/MuseScore_General.sf2
SCOREKIT_DEFAULT_SOUNDFONT_SOURCE ?=

.PHONY: all help build check doctor install install-bin install-sfizz install-skill install-sound-dir install-default-soundfont test-install sfizz book book-serve

all: build

help:
	@printf '%s\n' \
	  'make build          Build the release binary' \
	  'make check          Run format, clippy, and all tests' \
	  'make doctor         Check external audio dependencies' \
	  'make install        Install tools, skill, directories, and MuseScore General' \
	  'make install-bin    Install only the scorekit binary' \
	  'make install-sfizz  Build if needed, then install sfizz_render' \
	  'make install-skill  Install only the Agent skill' \
	  'make install-sound-dir  Create sf2/sfz/profiles sound directories' \
	  'make install-default-soundfont  Install MuseScore_General.sf2' \
	  'make test-install   Smoke-test install and failure recovery under target/' \
	  'make sfizz          Build assets/bin/sfizz_render from source' \
	  'make book           Build the English mdBook site' \
	  '' \
	  'Overrides: PREFIX=~/.local BINDIR=<dir> SKILLS_DIR=~/.agents/skills SCOREKIT_SOUND_LIBRARY_DIR=<dir>'

build:
	$(CARGO) build --locked --release

check:
	$(CARGO) fmt --check
	$(CARGO) clippy --all-targets -- -D warnings
	$(CARGO) test

doctor:
	$(CARGO) run --quiet -- doctor

install: install-bin install-sfizz install-skill install-sound-dir install-default-soundfont
	@printf 'Installed scorekit to %s, sfizz_render to %s, Agent skill to %s, and MuseScore General under %s\n' \
	  '$(BINDIR)/scorekit' '$(BINDIR)/sfizz_render' '$(SKILLS_DIR)/scorekit' '$(SCOREKIT_SOUND_LIBRARY_DIR)'

install-bin: build
	@set -eu; \
	destination='$(DESTDIR)$(BINDIR)'; \
	mkdir -p "$$destination"; \
	temporary=$$(mktemp "$$destination/.scorekit.XXXXXX"); \
	trap 'if test -f "$$temporary"; then unlink "$$temporary"; fi' EXIT HUP INT TERM; \
	$(INSTALL) -m 0755 '$(SCOREKIT_BIN)' "$$temporary"; \
	mv -f "$$temporary" "$$destination/scorekit"; \
	trap - EXIT HUP INT TERM

install-sfizz:
	@if test ! -x '$(SFIZZ_BIN)'; then $(MAKE) sfizz; fi
	@set -eu; \
	test -x '$(SFIZZ_BIN)'; \
	destination='$(DESTDIR)$(BINDIR)'; \
	mkdir -p "$$destination"; \
	temporary=$$(mktemp "$$destination/.sfizz_render.XXXXXX"); \
	trap 'if test -f "$$temporary"; then unlink "$$temporary"; fi' EXIT HUP INT TERM; \
	$(INSTALL) -m 0755 '$(SFIZZ_BIN)' "$$temporary"; \
	mv -f "$$temporary" "$$destination/sfizz_render"; \
	trap - EXIT HUP INT TERM

install-skill:
	@set -eu; \
	test -f skills/scorekit/SKILL.md; \
	mkdir -p '$(SKILLS_DIR)'; \
	destination='$(SKILLS_DIR)/scorekit'; \
	staging=$$(mktemp -d '$(SKILLS_DIR)/.scorekit.XXXXXX'); \
	backup='$(SKILLS_DIR)/.scorekit.backup.'$$$$; \
	trap 'if test -d "$$staging"; then find "$$staging" -depth -delete; fi; if test -e "$$backup" && ! test -e "$$destination"; then mv "$$backup" "$$destination"; fi' EXIT HUP INT TERM; \
	$(COPY) -R skills/scorekit/. "$$staging/"; \
	if test -e "$$destination"; then mv "$$destination" "$$backup"; fi; \
	mv "$$staging" "$$destination"; \
	if test -d "$$backup"; then find "$$backup" -depth -delete; fi; \
	trap - EXIT HUP INT TERM

install-sound-dir:
	$(INSTALL) -d \
	  '$(DESTDIR)$(SCOREKIT_SOUND_LIBRARY_DIR)/sf2' \
	  '$(DESTDIR)$(SCOREKIT_SOUND_LIBRARY_DIR)/sfz' \
	  '$(DESTDIR)$(SCOREKIT_SOUND_LIBRARY_DIR)/profiles'

install-default-soundfont: install-sound-dir
	@if test -n '$(SCOREKIT_DEFAULT_SOUNDFONT_SOURCE)'; then \
	  set -eu; \
	  test -f '$(SCOREKIT_DEFAULT_SOUNDFONT_SOURCE)'; \
	  destination='$(DESTDIR)$(SCOREKIT_DEFAULT_SOUNDFONT)'; \
	  temporary=$$(mktemp "$$destination.part.XXXXXX"); \
	  trap 'if test -f "$$temporary"; then unlink "$$temporary"; fi' EXIT HUP INT TERM; \
	  $(INSTALL) -m 0644 '$(SCOREKIT_DEFAULT_SOUNDFONT_SOURCE)' "$$temporary"; \
	  mv -f "$$temporary" "$$destination"; \
	  trap - EXIT HUP INT TERM; \
	else \
	  SCOREKIT_SOUND_LIBRARY_DIR='$(DESTDIR)$(SCOREKIT_SOUND_LIBRARY_DIR)' \
	    ./scripts/fetch_default_soundfont.sh; \
	fi

test-install:
	$(MAKE) install \
	  PREFIX='$(CURDIR)/$(TARGET_DIR)/install-test/prefix' \
	  SKILLS_DIR='$(CURDIR)/$(TARGET_DIR)/install-test/skills' \
	  SFIZZ_BIN='$(CURDIR)/$(SCOREKIT_BIN)' \
	  SCOREKIT_SOUND_LIBRARY_DIR='$(CURDIR)/$(TARGET_DIR)/install-test/sound-library' \
	  SCOREKIT_DEFAULT_SOUNDFONT_SOURCE='$(CURDIR)/assets/TimGM6mb.sf2'
	cmp '$(SCOREKIT_BIN)' '$(TARGET_DIR)/install-test/prefix/bin/scorekit'
	cmp '$(SCOREKIT_BIN)' '$(TARGET_DIR)/install-test/prefix/bin/sfizz_render'
	test -f '$(TARGET_DIR)/install-test/skills/scorekit/SKILL.md'
	test -f '$(TARGET_DIR)/install-test/skills/scorekit/reference.md'
	test -d '$(TARGET_DIR)/install-test/sound-library/sf2'
	test -d '$(TARGET_DIR)/install-test/sound-library/sfz'
	test -d '$(TARGET_DIR)/install-test/sound-library/profiles'
	cmp 'assets/TimGM6mb.sf2' '$(TARGET_DIR)/install-test/sound-library/sf2/MuseScore_General.sf2'
	@if $(MAKE) install-bin INSTALL=false \
	  PREFIX='$(CURDIR)/$(TARGET_DIR)/install-test/prefix' >/dev/null 2>&1; then \
	  printf '%s\n' 'expected install-bin failure' >&2; exit 1; \
	fi
	cmp '$(SCOREKIT_BIN)' '$(TARGET_DIR)/install-test/prefix/bin/scorekit'
	@if $(MAKE) install-sfizz INSTALL=false \
	  SFIZZ_BIN='$(CURDIR)/$(SCOREKIT_BIN)' \
	  PREFIX='$(CURDIR)/$(TARGET_DIR)/install-test/prefix' >/dev/null 2>&1; then \
	  printf '%s\n' 'expected install-sfizz failure' >&2; exit 1; \
	fi
	cmp '$(SCOREKIT_BIN)' '$(TARGET_DIR)/install-test/prefix/bin/sfizz_render'
	@if $(MAKE) install-skill COPY=false \
	  SKILLS_DIR='$(CURDIR)/$(TARGET_DIR)/install-test/skills' >/dev/null 2>&1; then \
	  printf '%s\n' 'expected install-skill failure' >&2; exit 1; \
	fi
	test -f '$(TARGET_DIR)/install-test/skills/scorekit/SKILL.md'
	test -f '$(TARGET_DIR)/install-test/skills/scorekit/reference.md'
	@if $(MAKE) install-default-soundfont INSTALL=false \
	  SCOREKIT_SOUND_LIBRARY_DIR='$(CURDIR)/$(TARGET_DIR)/install-test/sound-library' \
	  SCOREKIT_DEFAULT_SOUNDFONT_SOURCE='$(CURDIR)/assets/TimGM6mb.sf2' >/dev/null 2>&1; then \
	  printf '%s\n' 'expected install-default-soundfont failure' >&2; exit 1; \
	fi
	cmp 'assets/TimGM6mb.sf2' '$(TARGET_DIR)/install-test/sound-library/sf2/MuseScore_General.sf2'
	@set -eu; \
	fetch_dir='$(CURDIR)/$(TARGET_DIR)/install-test/fetch-library'; \
	rm -rf "$$fetch_dir"; \
	if command -v shasum >/dev/null 2>&1; then \
	  sha=$$(shasum -a 256 assets/TimGM6mb.sf2 | awk '{print $$1}'); \
	else \
	  sha=$$(sha256sum assets/TimGM6mb.sf2 | awk '{print $$1}'); \
	fi; \
	SCOREKIT_SOUND_LIBRARY_DIR="$$fetch_dir" \
	SCOREKIT_SOUNDFONT_URL='file://$(CURDIR)/assets/TimGM6mb.sf2' \
	SCOREKIT_SOUNDFONT_LICENSE_URL='file://$(CURDIR)/LICENSE' \
	SCOREKIT_SOUNDFONT_SHA256="$$sha" \
	  ./scripts/fetch_default_soundfont.sh >/dev/null; \
	cmp assets/TimGM6mb.sf2 "$$fetch_dir/sf2/MuseScore_General.sf2"; \
	cmp LICENSE "$$fetch_dir/sf2/MuseScore_General_License.md"
	@set -eu; \
	fetch_dir='$(CURDIR)/$(TARGET_DIR)/install-test/fetch-bad'; \
	rm -rf "$$fetch_dir"; \
	if SCOREKIT_SOUND_LIBRARY_DIR="$$fetch_dir" \
	  SCOREKIT_SOUNDFONT_URL='file://$(CURDIR)/assets/TimGM6mb.sf2' \
	  SCOREKIT_SOUNDFONT_LICENSE_URL='file://$(CURDIR)/LICENSE' \
	  SCOREKIT_SOUNDFONT_SHA256='0000000000000000000000000000000000000000000000000000000000000000' \
	  ./scripts/fetch_default_soundfont.sh >/dev/null 2>&1; then \
	  printf '%s\n' 'expected fetch checksum failure' >&2; exit 1; \
	fi; \
	test ! -f "$$fetch_dir/sf2/MuseScore_General.sf2"; \
	test -z "$$(find "$$fetch_dir" -name '*.part' -print -quit)"
	test -z "$$(find '$(TARGET_DIR)/install-test' \( -name '.scorekit*' -o -name '.sfizz_render*' -o -name '*.part' -o -name '*.part.*' \) -print -quit)"

sfizz:
	./scripts/build_sfizz.sh

book:
	$(MDBOOK) build docs-site

book-serve:
	$(MDBOOK) serve docs-site --open
