#!/bin/sh
# The archiver half of `stub-cc.sh`: makes whatever archive it is asked for,
# empty, in either `ar` (`ar cq out.a ...`) or `lib.exe` (`-out:out.lib`)
# style. Same purpose and the same limit — nothing built with it links.
for a in "$@"; do
	case "$a" in
	-out:* | /OUT:* | /out:*) : >"${a#*:}"; exit 0 ;;
	*.a | *.lib) : >"$a"; exit 0 ;;
	esac
done
exit 0
