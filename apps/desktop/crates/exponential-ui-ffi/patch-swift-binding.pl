#!/usr/bin/env perl
# VAPP-131: post-process the GENERATED Swift binding (generate-bindings.sh and
# build-ios.sh run it after uniffi-bindgen, so regeneration keeps it): uniffi's
# reader copies every integer and every string's bytes out of the `Data` into
# a fresh buffer (61% of the SwiftUI template-10k profile). The patched
# readers load in place from the buffer, with the same bounds checks. Rust
# strings are always valid UTF-8, so `String(decoding:)` (no validation
# failure path) lifts them like `String(bytes:encoding:)!` did.
# A uniffi upgrade that changes the generated text fails here loudly
# instead of silently dropping the patch.
#   perl patch-swift-binding.pl <ExponentialUIFFI.swift>
use strict;
use warnings;

my $path = shift or die "usage: patch-swift-binding.pl <binding.swift>\n";
open(my $in, '<', $path) or die "$path: $!\n";
my $src = do { local $/; <$in> };
close $in;

my @patches = (
  [ 'readInt', <<'OLD', <<'NEW' ],
    if T.self == UInt8.self {
        let value = reader.data[reader.offset]
        reader.offset += 1
        return value as! T
    }
    var value: T = 0
    let _ = withUnsafeMutableBytes(of: &value, { reader.data.copyBytes(to: $0, from: range)})
    reader.offset = range.upperBound
    return value.bigEndian
OLD
    let offset = reader.offset
    let value = reader.data.withUnsafeBytes { $0.loadUnaligned(fromByteOffset: offset, as: T.self) }
    reader.offset = range.upperBound
    return T(bigEndian: value)
NEW
  [ 'readBytes', <<'OLD', <<'NEW' ],
    var value = [UInt8](repeating: 0, count: count)
    value.withUnsafeMutableBufferPointer({ buffer in
        reader.data.copyBytes(to: buffer, from: range)
    })
    reader.offset = range.upperBound
    return value
OLD
    let value = reader.data.withUnsafeBytes { Array($0[range]) }
    reader.offset = range.upperBound
    return value
NEW
  [ 'FfiConverterString.read', <<'OLD', <<'NEW' ],
        let len: Int32 = try readInt(&buf)
        return String(bytes: try readBytes(&buf, count: Int(len)), encoding: String.Encoding.utf8)!
OLD
        let len: Int32 = try readInt(&buf)
        let range = buf.offset..<(buf.offset + Int(len))
        guard buf.data.count >= range.upperBound else {
            throw UniffiInternalError.bufferOverflow
        }
        let value = buf.data.withUnsafeBytes { String(decoding: $0[range], as: UTF8.self) }
        buf.offset = range.upperBound
        return value
NEW
);

for my $p (@patches) {
  my ($name, $old, $new) = @$p;
  my $at = index($src, $old);
  if ($at < 0) {
    # Already patched (a second run) is fine; anything else is a new uniffi.
    next if index($src, $new) >= 0;
    die "patch-swift-binding: the generated $name changed (uniffi upgrade?): update $0\n";
  }
  die "patch-swift-binding: $name occurs twice\n" if index($src, $old, $at + 1) >= 0;
  substr($src, $at, length($old)) = $new;
}

open(my $out, '>', $path) or die "$path: $!\n";
print $out $src;
close $out;
print "patched the Swift binding readers: $path\n";
