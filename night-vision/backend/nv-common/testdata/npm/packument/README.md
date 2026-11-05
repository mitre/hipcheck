# Real-world npm packument corpus

`corpus.json` is the reviewed catalog for registry-derived fixtures in this
directory. Each entry records its source and capture date. Keep small,
hand-written fixtures beside this corpus for individual rules and error
variants; do not replace them with these real-world inputs.

The files under `real/` retain the packument name, every dist tag, and the
version metadata targeted by those tags. They are purposely versioned,
deterministic regression inputs. Full registry responses are saved only under
the gitignored `cache/` directory for local investigation.

Run `cargo nvdb npm packument corpus-refresh --destructive` to fetch the fixed
package list and update `corpus-refresh.json`. It never overwrites these
reviewed responses, and is intended for scheduled diagnostics rather than CI.

To add a new reviewed fixture, run `cargo nvdb npm packument corpus-add
<PACKAGE> --destructive`. The command validates the registry response, refuses
duplicate package names and fixture paths, saves the full response in `cache/`,
writes a minimized fixture under `real/`, and adds the catalog entry for review.
