trash 5.2.9, vendored from crates.io. License: LICENSE.txt.

Disco patch to src/freedesktop.rs: split_once for .trashinfo key/value lines, skip blank or malformed lines, preserve equals characters in paths, and skip paths without a parent rather than panicking. All other behavior remains upstream.
