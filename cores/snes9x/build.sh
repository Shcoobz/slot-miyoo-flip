#!/bin/sh
# Snes9x, the full SNES core the Miyoo Flip build offers beside Snes9x 2005 Plus (its default),
# built from its pinned source archive.
# Snes9x is non-commercial freeware; the archive ships beside it as its source.
set -eu

device_cflags="-mcpu=cortex-a55 -O2 -DNDEBUG"

usage() {
	echo "usage: $0 stamp COMMIT | build COMMIT TARBALL WORKDIR OUT" >&2
	exit 2
}

stamp() {
	echo "commit=$1"
	echo "source=https://github.com/snes9xgit/snes9x/archive/$1.tar.gz"
	echo "recipe=make -C libretro platform=unix"
	echo "device_cflags=$device_cflags"
}

build() {
	commit="$1" tarball="$2" work="$3" out="$4"
	src="$work/snes9x-$commit"

	rm -rf "$work"
	mkdir -p "$work"
	tar -xzf "$tarball" -C "$work"
	if [ ! -f "$src/libretro/Makefile" ]; then
		echo "$tarball does not hold snes9x-$commit/libretro/Makefile" >&2
		exit 1
	fi

	# The archive is no git checkout, so name the commit rather than let the Makefile ask git.
	# Its sources use C++17 (std::string_view), which gcc 10 does not default to.
	CFLAGS="$device_cflags" CXXFLAGS="$device_cflags -std=c++17" \
		make -C "$src/libretro" platform=unix \
		GIT_VERSION="\" $(printf %s "$commit" | cut -c1-7)\"" \
		-j"$(getconf _NPROCESSORS_ONLN)"

	mkdir -p "$(dirname "$out")"
	cp "$src/libretro/snes9x_libretro.so" "$out"
	stamp "$commit" >"$out.meta"
}

case "${1:-}" in
stamp)
	[ $# -eq 2 ] && [ -n "$2" ] || usage
	stamp "$2"
	;;
build)
	[ $# -eq 5 ] && [ -n "$2" ] && [ -n "$3" ] && [ -n "$4" ] && [ -n "$5" ] || usage
	build "$2" "$3" "$4" "$5"
	;;
*)
	usage
	;;
esac
