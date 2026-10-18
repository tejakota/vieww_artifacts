#!/usr/bin/env bash
# The conformance matrix's gate: vieww's own row, diffed against the merge
# base with origin/main, plus the why-not contract.
#
# docs/conformance/matrix.md carries vieww's row twice — as prose and as a
# machine-readable `<!-- conformance-row ... -->` block — and this script
# enforces two things about it:
#
# 1. **A changed row is a decision, not a drift.** If the block differs from
#    the base's, the commits being gated must carry a `Matrix-Changed:`
#    trailer — the same protocol the raster-identity gate runs
#    (`docs/FIDELITY.md`'s `Pixels-Changed:`): the trailer carries the
#    decision, the gate surfaces it, and a silent cell flip is a failure
#    rather than a README edit nobody read.
# 2. **Every non-true cell is planned.** Each `false`/`partial` line in the
#    block must have a matching section in `docs/conformance/why-not.md`,
#    whose first-level sections are exactly the planned cells. A false cell
#    without a plan is a gap pretending to be a choice; the file's own rule
#    3 says so, and this is the teeth.
#
# Like the raster-identity stage: a shallow clone that cannot name a merge
# base skips loudly, and the row-diff half is skipped without complaint when
# the base predates the matrix (its introduction is not a change to gate).

set -u
# Resolve from the script's own location, then re-root at the repository
# root: `git show base:path` takes paths relative to the root, and this
# file lives one directory below it (`vieww_base/`), so the matrix's
# root-relative path is the checkout prefix plus the in-checkout path.
here="$(cd "$(dirname "$0")" && pwd)"
prefix="$(git -C "$here/../.." rev-parse --show-prefix)"
root="$(git -C "$here/../.." rev-parse --show-toplevel)"
cd "$root" || exit 1

matrix="${prefix}docs/conformance/matrix.md"
why_not="${prefix}docs/conformance/why-not.md"
rc=0

row() {
	# Everything between the conformance-row marker's opening line and the
	# closing '-->', as `key=value` lines. Only the marker block — the prose
	# table above it is prose.
	sed -n '/<!-- conformance-row/,/-->/p' "$matrix" \
		| grep -E '^[a-z_]+=(true|false|partial)$'
}

base="${1:-}"
if [ -z "$base" ]; then
	if origin="$(git rev-parse -q --verify origin/main)"; then
		base="$(git merge-base HEAD "$origin")"
	fi
fi

if ! current="$(row)"; then
	echo "conformance-matrix: $matrix has no parseable conformance-row block"
	echo "  (the block at the bottom: '<!-- conformance-row ... -->' with key=value lines)"
	exit 1
fi

if [ -z "$base" ]; then
	echo "conformance-matrix: row-diff skipped — origin/main resolves to no commit in this checkout"
	echo "  a shallow clone cannot name a BASE; fetch it: git fetch origin main"
elif git cat-file -e "$base:$matrix" 2>/dev/null; then
	base_row="$(git show "$base:$matrix" | sed -n '/<!-- conformance-row/,/-->/p' | grep -E '^[a-z_]+=(true|false|partial)$' || true)"
	if [ "$current" != "$base_row" ]; then
		if git log --format=%B "$base..HEAD" | grep -q '^Matrix-Changed:'; then
			echo 'conformance-matrix: the row changed and a Matrix-Changed: trailer carries the decision — reporting, not failing'
			diff <(printf '%s\n' "$base_row") <(printf '%s\n' "$current") | sed 's/^/  /'
		else
			echo "conformance-matrix: vieww's row changed against $base without a Matrix-Changed: trailer"
			diff <(printf '%s\n' "$base_row") <(printf '%s\n' "$current") | sed 's/^/  /'
			echo '  If the change is deliberate: add `Matrix-Changed: <what and why>` to the commit message,'
			echo '  and give every newly false cell a plan in docs/conformance/why-not.md.'
			rc=1
		fi
	fi
else
	echo "conformance-matrix: the matrix is new at this base — row established, nothing to diff"
fi

# The why-not contract: every false/partial cell needs a first-level
# section in why-not.md (`## cell-name`), and every such section needs a
# cell that is actually false — a plan for a solved problem is worse than
# none, because it makes the matrix lie in the other direction.
unplanned=$(printf '%s\n' "$current" | awk -F= '$2 != "true" {print $1}' | while read -r cell; do
	if ! grep -q "^## $cell" "$why_not"; then
		echo "$cell"
	fi
done)
if [ -n "$unplanned" ]; then
	echo "conformance-matrix: cells are false/partial with no plan in $why_not:"
	printf '%s\n' "$unplanned" | sed 's/^/  /'
	rc=1
fi

orphaned=$(grep -E '^## ' "$why_not" | sed 's/^## //' | awk '{print $1}' | while read -r cell; do
	if ! printf '%s\n' "$current" | grep -Eq "^$cell=(false|partial)$"; then
		echo "$cell"
	fi
done)
if [ -n "$orphaned" ]; then
	echo "conformance-matrix: $why_not plans cells that are not false/partial in the row (delete them):"
	printf '%s\n' "$orphaned" | sed 's/^/  /'
	rc=1
fi

if [ "$rc" -eq 0 ]; then
	echo "conformance-matrix: row consistent with the base and fully planned"
fi
exit "$rc"
