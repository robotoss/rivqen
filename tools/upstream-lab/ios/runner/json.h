// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// JSON output of the iOS function runner (WP-01). Foundation only: no upstream header, so
// check.sh can build and test it without UPSTREAM.

#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

// An object whose keys are written in the order of `keys` (the order the SDK returned).
@interface RQOrdered : NSObject
@property(nonatomic, copy) NSArray<NSString *> *keys;
@property(nonatomic, copy) NSDictionary<NSString *, id> *values;
@end

// The order the SDK dictionary reports its keys in (not a defined order, see README).
RQOrdered *rqKeepOrder(NSDictionary *d);

// One compact JSON text: sorted keys for NSDictionary, `keys` order for RQOrdered, UTF-8 text.
// A number that is not finite has no JSON form: it is written as the string "nan", "inf" or
// "-inf", never as a bare token. The runner does not let such a result reach the writer
// (rqNonFinite); this rule only keeps the output valid JSON in every case.
NSString *rqLine(id _Nullable v);

// The JSON Pointer (RFC 6901) under `root` of the first number in `v` that is not finite, in the
// write order, or nil when every number is finite. `token` gets "nan", "inf" or "-inf".
NSString *_Nullable rqNonFinite(id _Nullable v, NSString *root, NSString *_Nullable *_Nullable token);

NS_ASSUME_NONNULL_END
