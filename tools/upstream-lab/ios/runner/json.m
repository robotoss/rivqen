// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// JSON output of the iOS function runner (WP-01). Own writer: sorted keys, UTF-8, and an ordered
// object type for results whose key order is the order the SDK returned. See json.h.

#import "json.h"

#include <math.h>

@implementation RQOrdered
@end

RQOrdered *rqKeepOrder(NSDictionary *d) {
  RQOrdered *o = [RQOrdered new];
  o.keys = d.allKeys;
  o.values = d;
  return o;
}

static BOOL rqIsBool(NSNumber *n) { return CFGetTypeID((__bridge CFTypeRef)n) == CFBooleanGetTypeID(); }

// "nan", "inf" or "-inf" for a float number that is not finite; nil for every other number.
static NSString *_Nullable rqNonFiniteToken(NSNumber *n) {
  if (rqIsBool(n) || !CFNumberIsFloatType((__bridge CFNumberRef)n)) return nil;
  double d = n.doubleValue;
  if (isnan(d)) return @"nan";
  if (isinf(d)) return d > 0 ? @"inf" : @"-inf";
  return nil;
}

static NSArray *rqSortedKeys(NSDictionary *d) { return [d.allKeys sortedArrayUsingSelector:@selector(compare:)]; }

static void rqWriteString(NSMutableString *out, NSString *s) {
  [out appendString:@"\""];
  for (NSUInteger i = 0; i < s.length; i++) {
    unichar c = [s characterAtIndex:i];
    switch (c) {
      case '"': [out appendString:@"\\\""]; break;
      case '\\': [out appendString:@"\\\\"]; break;
      case '\n': [out appendString:@"\\n"]; break;
      case '\r': [out appendString:@"\\r"]; break;
      case '\t': [out appendString:@"\\t"]; break;
      case '\b': [out appendString:@"\\b"]; break;
      case '\f': [out appendString:@"\\f"]; break;
      default:
        if (c < 0x20) {
          [out appendFormat:@"\\u%04x", c];
        } else if (CFStringIsSurrogateHighCharacter(c) && i + 1 < s.length &&
                   CFStringIsSurrogateLowCharacter([s characterAtIndex:i + 1])) {
          // A surrogate pair is one character: write both units as text.
          [out appendFormat:@"%C%C", c, [s characterAtIndex:i + 1]];
          i++;
        } else if (CFStringIsSurrogateHighCharacter(c) || CFStringIsSurrogateLowCharacter(c)) {
          // A lone surrogate has no UTF-8 form: escape it, so the line stays writable JSON.
          [out appendFormat:@"\\u%04x", c];
        } else {
          [out appendFormat:@"%C", c];
        }
    }
  }
  [out appendString:@"\""];
}

static void rqWrite(NSMutableString *out, id _Nullable v);

static void rqWriteObject(NSMutableString *out, NSArray<NSString *> *keys, NSDictionary *d) {
  [out appendString:@"{"];
  BOOL first = YES;
  for (NSString *k in keys) {
    if (!first) [out appendString:@","];
    first = NO;
    rqWriteString(out, k);
    [out appendString:@":"];
    rqWrite(out, d[k]);
  }
  [out appendString:@"}"];
}

static void rqWrite(NSMutableString *out, id _Nullable v) {
  if (v == nil || [v isKindOfClass:[NSNull class]]) {
    [out appendString:@"null"];
  } else if ([v isKindOfClass:[NSString class]]) {
    rqWriteString(out, v);
  } else if ([v isKindOfClass:[NSNumber class]]) {
    NSNumber *n = v;
    NSString *token = rqNonFiniteToken(n);
    if (rqIsBool(n)) {
      [out appendString:n.boolValue ? @"true" : @"false"];
    } else if (token != nil) {
      // JSON has no form for this number; a bare nan or inf would make the line invalid.
      rqWriteString(out, token);
    } else if (CFNumberIsFloatType((__bridge CFNumberRef)n)) {
      [out appendFormat:@"%.17g", n.doubleValue];
    } else {
      [out appendFormat:@"%lld", n.longLongValue];
    }
  } else if ([v isKindOfClass:[NSArray class]]) {
    [out appendString:@"["];
    BOOL first = YES;
    for (id e in (NSArray *)v) {
      if (!first) [out appendString:@","];
      first = NO;
      rqWrite(out, e);
    }
    [out appendString:@"]"];
  } else if ([v isKindOfClass:[RQOrdered class]]) {
    RQOrdered *o = v;
    rqWriteObject(out, o.keys, o.values);
  } else if ([v isKindOfClass:[NSDictionary class]]) {
    NSDictionary *d = v;
    rqWriteObject(out, rqSortedKeys(d), d);
  } else {
    rqWriteString(out, [v description]);
  }
}

NSString *rqLine(id _Nullable v) {
  NSMutableString *s = [NSMutableString string];
  rqWrite(s, v);
  return s;
}

// RFC 6901 reference token: "~" is "~0", "/" is "~1".
static NSString *rqPointer(NSString *base, id key) {
  NSString *k = [key isKindOfClass:[NSString class]] ? key : [key description];
  k = [k stringByReplacingOccurrencesOfString:@"~" withString:@"~0"];
  k = [k stringByReplacingOccurrencesOfString:@"/" withString:@"~1"];
  return [NSString stringWithFormat:@"%@/%@", base, k];
}

static NSString *_Nullable rqFindInObject(NSArray *keys, NSDictionary *d, NSString *at, NSString *_Nullable *_Nullable token) {
  for (id k in keys) {
    NSString *found = rqNonFinite(d[k], rqPointer(at, k), token);
    if (found) return found;
  }
  return nil;
}

NSString *_Nullable rqNonFinite(id _Nullable v, NSString *root, NSString *_Nullable *_Nullable token) {
  if ([v isKindOfClass:[NSNumber class]]) {
    NSString *t = rqNonFiniteToken(v);
    if (t == nil) return nil;
    if (token) *token = t;
    return root;
  }
  if ([v isKindOfClass:[NSArray class]]) {
    NSArray *a = v;
    for (NSUInteger i = 0; i < a.count; i++) {
      NSString *found = rqNonFinite(a[i], rqPointer(root, @(i)), token);
      if (found) return found;
    }
    return nil;
  }
  if ([v isKindOfClass:[RQOrdered class]]) {
    RQOrdered *o = v;
    return rqFindInObject(o.keys, o.values, root, token);
  }
  if ([v isKindOfClass:[NSDictionary class]]) {
    return rqFindInObject(rqSortedKeys(v), v, root, token);
  }
  return nil;
}
