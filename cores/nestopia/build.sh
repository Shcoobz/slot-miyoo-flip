#!/bin/sh
# Nestopia, the alternative NES core on the Miyoo Flip, built from its pinned source archive.
# The archive ships beside it as its GPL-2.0 corresponding source.
set -eu

device_cflags="-mcpu=cortex-a55 -O2 -DNDEBUG"

usage() {
	echo "usage: $0 stamp COMMIT | build COMMIT TARBALL WORKDIR OUT" >&2
	exit 2
}

stamp() {
	echo "commit=$1"
	echo "source=https://github.com/libretro/nestopia/archive/$1.tar.gz"
	echo "recipe=make -C libretro platform=unix"
	echo "device_cflags=$device_cflags"
}

build() {
	commit="$1" tarball="$2" work="$3" out="$4"
	src="$work/nestopia-$commit"

	rm -rf "$work"
	mkdir -p "$work"
	tar -xzf "$tarball" -C "$work"
	if [ ! -f "$src/libretro/Makefile" ]; then
		echo "$tarball does not hold nestopia-$commit/libretro/Makefile" >&2
		exit 1
	fi

	# The archive is no git checkout, so name the commit rather than let the Makefile ask git.
	CFLAGS="$device_cflags" CXXFLAGS="$device_cflags" \
		make -C "$src/libretro" platform=unix \
		GIT_VERSION="\" $(printf %s "$commit" | cut -c1-7)\"" \
		-j"$(getconf _NPROCESSORS_ONLN)"

	mkdir -p "$(dirname "$out")"
	cp "$src/libretro/nestopia_libretro.so" "$out"
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
