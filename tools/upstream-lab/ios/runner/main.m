// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Rivqen iOS function runner for macOS (WP-01 S1, H-119).
// Reads a CONTRACT section 7 case file, calls the pure functions of the unmodified
// iOS SDK, and writes one JSON line per case. No network. No session.
// Usage: rivqen-ios-fn <cases.json> <out.jsonl> <out.meta.json>

#import <Foundation/Foundation.h>

#import "SonicConnection.h"
#import "SonicEngine.h"
#import "SonicServer.h"
#import "SonicSession.h"
#import "SonicUtil.h"

// Stand-in for the UIKit name that the upstream cache file imports (see shim/UIKit/UIKit.h).
NSString *const UIApplicationDidReceiveMemoryWarningNotification = @"UIApplicationDidReceiveMemoryWarningNotification";

// A method that the upstream server class defines but does not declare in its header.
// Only a declaration: nothing of the upstream class is changed or reached by reflection.
@interface SonicServer (RivqenLab)
- (NSStringEncoding)encodingFromHeaders;
@end

// ---------------------------------------------------------------------------
// JSON output. Own writer: sorted keys, UTF-8, and an ordered object type for
// results whose key order is the order the SDK returned.
// ---------------------------------------------------------------------------

@interface RQOrdered : NSObject
@property(nonatomic, copy) NSArray<NSString *> *keys;
@property(nonatomic, copy) NSDictionary<NSString *, id> *values;
@end
@implementation RQOrdered
@end

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
        } else {
          [out appendFormat:@"%C", c];
        }
    }
  }
  [out appendString:@"\""];
}

static void rqWrite(NSMutableString *out, id v);

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

static void rqWrite(NSMutableString *out, id v) {
  if (v == nil || [v isKindOfClass:[NSNull class]]) {
    [out appendString:@"null"];
  } else if ([v isKindOfClass:[NSString class]]) {
    rqWriteString(out, v);
  } else if ([v isKindOfClass:[NSNumber class]]) {
    NSNumber *n = v;
    if (CFGetTypeID((__bridge CFTypeRef)n) == CFBooleanGetTypeID()) {
      [out appendString:n.boolValue ? @"true" : @"false"];
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
    rqWriteObject(out, [d.allKeys sortedArrayUsingSelector:@selector(compare:)], d);
  } else {
    rqWriteString(out, [v description]);
  }
}

static NSString *rqLine(id v) {
  NSMutableString *s = [NSMutableString string];
  rqWrite(s, v);
  return s;
}

// The order the SDK dictionary reports its keys in (not a defined order, see README).
static RQOrdered *rqKeepOrder(NSDictionary *d) {
  RQOrdered *o = [RQOrdered new];
  o.keys = d.allKeys;
  o.values = d;
  return o;
}

// ---------------------------------------------------------------------------
// Input access with typed errors.
// ---------------------------------------------------------------------------

@interface RQError : NSObject
@property(nonatomic, copy) NSString *message;
@end
@implementation RQError
+ (instancetype)with:(NSString *)m {
  RQError *e = [RQError new];
  e.message = m;
  return e;
}
@end

static NSString *rqString(NSDictionary *in, NSString *key, RQError **err) {
  id v = in[key];
  if (![v isKindOfClass:[NSString class]]) {
    *err = [RQError with:[NSString stringWithFormat:@"input.%@ must be a string", key]];
    return nil;
  }
  return v;
}

static NSDictionary *rqObject(NSDictionary *in, NSString *key, RQError **err) {
  id v = in[key];
  if (![v isKindOfClass:[NSDictionary class]]) {
    *err = [RQError with:[NSString stringWithFormat:@"input.%@ must be an object", key]];
    return nil;
  }
  return v;
}

// ---------------------------------------------------------------------------
// Server response helper: the response goes through the public connection delegate
// entry point of the SDK server, as a network response would. The SDK lowercases
// the field names and normalizes a weak entity tag there.
// ---------------------------------------------------------------------------

static SonicServer *rqServerWithFields(NSDictionary *fields, RQError **err) {
  for (id k in fields) {
    if (![k isKindOfClass:[NSString class]] || ![fields[k] isKindOfClass:[NSString class]]) {
      *err = [RQError with:@"input.fields must map strings to strings"];
      return nil;
    }
  }
  SonicServer *server = [[SonicServer alloc] initWithUrl:@"http://example.com/p" delegate:nil delegateQueue:nil];
  NSHTTPURLResponse *response = [[NSHTTPURLResponse alloc] initWithURL:[NSURL URLWithString:@"http://example.com/p"]
                                                            statusCode:200
                                                           HTTPVersion:@"HTTP/1.1"
                                                          headerFields:fields];
  [(id<SonicConnectionDelegate>)server connection:nil didReceiveResponse:response];
  [[SonicSession sonicSessionQueue] waitUntilAllOperationsAreFinished];
  return server;
}

// ---------------------------------------------------------------------------
// Functions.
// ---------------------------------------------------------------------------

typedef NSDictionary *(^RQFn)(NSDictionary *in, RQError **err);

static NSDictionary *fnSessionId(NSDictionary *in, RQError **err) {
  NSString *url = rqString(in, @"url", err);
  if (!url) return nil;
  id related = in[@"account_related"];
  if (related != nil && ![related isKindOfClass:[NSNumber class]]) {
    *err = [RQError with:@"input.account_related must be a boolean"];
    return nil;
  }
  id account = in[@"account"];
  if ([related boolValue] && [account isKindOfClass:[NSString class]] && [account length] > 0) {
    [[SonicEngine sharedEngine] setCurrentUserAccount:account];
  }
  NSString *sid = sonicSessionID(url);
  return @{@"id" : sid ?: [NSNull null]};
}

static NSDictionary *fnSplit(NSDictionary *in, RQError **err) {
  NSString *html = rqString(in, @"html", err);
  if (!html) return nil;
  NSDictionary *r = [SonicUtil splitTemplateAndDataFromHtmlData:html];
  NSDictionary *data = r[@"data"];
  return @{@"template" : r[@"template"] ?: [NSNull null], @"data" : data ? rqKeepOrder(data) : [NSNull null]};
}

static NSDictionary *fnDiff(NSDictionary *in, RQError **err) {
  NSDictionary *server = rqObject(in, @"server", err);
  if (!server) return nil;
  NSMutableDictionary *local = nil;
  if (![in[@"local"] isKindOfClass:[NSNull class]]) {
    NSDictionary *l = rqObject(in, @"local", err);
    if (!l) return nil;
    local = [l mutableCopy];
  }
  NSDictionary *r = [SonicUtil mergeDynamicData:server withOriginData:local];
  NSDictionary *diff = r[@"diff"];
  return @{@"diff" : diff ? rqKeepOrder(diff) : [NSNull null]};
}

static NSDictionary *fnIsLegacy(NSDictionary *in, RQError **err) {
  NSDictionary *fields = rqObject(in, @"fields", err);
  if (!fields) return nil;
  SonicServer *server = rqServerWithFields(fields, err);
  if (!server) return nil;
  return @{@"value" : @([server isSonicResponse])};
}

static NSDictionary *fnEtag(NSDictionary *in, RQError **err) {
  NSString *value = rqString(in, @"value", err);
  if (!value) return nil;
  SonicServer *server = rqServerWithFields(@{@"ETag" : value}, err);
  if (!server) return nil;
  return @{@"value" : [server responseHeaderForKey:@"etag"] ?: [NSNull null]};
}

static NSDictionary *fnCharset(NSDictionary *in, RQError **err) {
  NSString *ct = rqString(in, @"content_type", err);
  if (!ct) return nil;
  SonicServer *server = rqServerWithFields(@{@"Content-Type" : ct}, err);
  if (!server) return nil;
  NSStringEncoding enc = [server encodingFromHeaders];
  CFStringRef name = CFStringConvertEncodingToIANACharSetName(CFStringConvertNSStringEncodingToEncoding(enc));
  return @{
    @"value" : name ? (__bridge NSString *)name : [NSNull null],
    @"ns_string_encoding" : @(enc),
  };
}

static NSDictionary<NSString *, NSString *> *rqUnsupported(void) {
  return @{
    @"message" : @"The page message is built by a method of the session class from private session state "
                 @"(status codes, server object, read-only diff data) that has no public setter. A "
                 @"session needs the engine and the web view delegate. Stays code reading.",
    @"template_change" : @"The template-change test is a private method of the session class that reads the "
                         @"private server property of a session. No public entry point. Stays code reading.",
  };
}

// ---------------------------------------------------------------------------
// Driver.
// ---------------------------------------------------------------------------

static BOOL rqActiveAccount(NSDictionary *c) {
  if (![c[@"fn"] isEqual:@"session_id"]) return NO;
  NSDictionary *in = c[@"input"];
  return [in isKindOfClass:[NSDictionary class]] && [in[@"account_related"] boolValue] &&
         [in[@"account"] isKindOfClass:[NSString class]] && [in[@"account"] length] > 0;
}

static NSDictionary *rqRunCase(NSDictionary *c, NSMutableSet<NSString *> *used) {
  NSString *cid = [c[@"id"] isKindOfClass:[NSString class]] ? c[@"id"] : @"";
  NSString *fn = [c[@"fn"] isKindOfClass:[NSString class]] ? c[@"fn"] : @"";
  NSMutableDictionary *rec = [@{@"id" : cid, @"fn" : fn, @"platform" : @"ios"} mutableCopy];
  NSDictionary *table = @{
    @"session_id" : ^NSDictionary *(NSDictionary *i, RQError **e) { return fnSessionId(i, e); },
    @"split" : ^NSDictionary *(NSDictionary *i, RQError **e) { return fnSplit(i, e); },
    @"diff" : ^NSDictionary *(NSDictionary *i, RQError **e) { return fnDiff(i, e); },
    @"is_legacy_response" : ^NSDictionary *(NSDictionary *i, RQError **e) { return fnIsLegacy(i, e); },
    @"etag" : ^NSDictionary *(NSDictionary *i, RQError **e) { return fnEtag(i, e); },
    @"charset" : ^NSDictionary *(NSDictionary *i, RQError **e) { return fnCharset(i, e); },
  };
  NSString *reason = rqUnsupported()[fn];
  if (reason) {
    rec[@"unsupported"] = reason;
    return rec;
  }
  RQFn f = table[fn];
  if (!f) {
    rec[@"unsupported"] = [NSString stringWithFormat:@"Unknown function '%@'", fn];
    return rec;
  }
  NSDictionary *in = c[@"input"];
  if (![in isKindOfClass:[NSDictionary class]]) {
    rec[@"error"] = @"case.input must be an object";
    return rec;
  }
  [used addObject:fn];
  RQError *err = nil;
  NSDictionary *result = nil;
  @try {
    result = f(in, &err);
  } @catch (NSException *ex) {
    err = [RQError with:[NSString stringWithFormat:@"%@: %@", ex.name, ex.reason]];
  }
  if (result) {
    rec[@"result"] = result;
  } else {
    rec[@"error"] = err.message ?: @"no result";
  }
  return rec;
}

static BOOL rqWriteFile(NSString *path, NSString *text) {
  NSError *error = nil;
  if (![text writeToFile:path atomically:YES encoding:NSUTF8StringEncoding error:&error]) {
    fprintf(stderr, "rivqen-ios-fn: cannot write %s: %s\n", path.UTF8String, error.localizedDescription.UTF8String);
    return NO;
  }
  return YES;
}

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc != 4) {
      fprintf(stderr, "usage: rivqen-ios-fn <cases.json> <out.jsonl> <out.meta.json>\n");
      return 2;
    }
    NSError *error = nil;
    NSData *raw = [NSData dataWithContentsOfFile:@(argv[1]) options:0 error:&error];
    if (!raw) {
      fprintf(stderr, "rivqen-ios-fn: cannot read %s: %s\n", argv[1], error.localizedDescription.UTF8String);
      return 2;
    }
    id parsed = [NSJSONSerialization JSONObjectWithData:raw options:0 error:&error];
    if (![parsed isKindOfClass:[NSArray class]]) {
      fprintf(stderr, "rivqen-ios-fn: %s is not a JSON array of cases (%s)\n", argv[1],
              error ? error.localizedDescription.UTF8String : "wrong type");
      return 2;
    }
    NSArray *cases = parsed;
    for (id c in cases) {
      if (![c isKindOfClass:[NSDictionary class]]) {
        fprintf(stderr, "rivqen-ios-fn: a case is not an object\n");
        return 2;
      }
    }

    // The SDK engine keeps the account once set and ignores an empty one. Run all cases without
    // an account first, then the session ids with an account (each sets its own account).
    NSMutableArray<NSNumber *> *order = [NSMutableArray array];
    for (NSUInteger i = 0; i < cases.count; i++)
      if (!rqActiveAccount(cases[i])) [order addObject:@(i)];
    for (NSUInteger i = 0; i < cases.count; i++)
      if (rqActiveAccount(cases[i])) [order addObject:@(i)];

    NSMutableSet<NSString *> *used = [NSMutableSet set];
    NSMutableArray *records = [NSMutableArray arrayWithCapacity:cases.count];
    for (NSUInteger i = 0; i < cases.count; i++) [records addObject:[NSNull null]];
    for (NSNumber *idx in order) records[idx.unsignedIntegerValue] = rqRunCase(cases[idx.unsignedIntegerValue], used);

    NSMutableString *jsonl = [NSMutableString string];
    for (id rec in records) [jsonl appendFormat:@"%@\n", rqLine(rec)];
    if (!rqWriteFile(@(argv[2]), jsonl)) return 1;

    NSMutableArray *calls = [NSMutableArray array];
    if ([used containsObject:@"is_legacy_response"] || [used containsObject:@"etag"] || [used containsObject:@"charset"]) {
      [calls addObject:@"SonicServer connection:didReceiveResponse: (SonicConnectionDelegate; public protocol)"];
      [calls addObject:@"SonicSession +sonicSessionQueue (public)"];
    }
    if ([used containsObject:@"charset"]) {
      [calls addObject:@"SonicServer -encodingFromHeaders (defined in SonicServer.m, not declared in its header; declared by the runner)"];
    }
    if ([used containsObject:@"session_id"]) {
      [calls addObject:@"SonicEngine -setCurrentUserAccount: and sonicSessionID() (public; the account setter makes a cache directory under CFFIXED_USER_HOME)"];
    }
    NSOperatingSystemVersion v = [NSProcessInfo processInfo].operatingSystemVersion;
    NSDictionary *meta = @{
      @"platform" : @"ios",
      @"runner" : @"macOS command-line runner (Foundation)",
      @"macos_version" : [NSString stringWithFormat:@"%ld.%ld.%ld", (long)v.majorVersion, (long)v.minorVersion, (long)v.patchVersion],
      @"macos_build" : [NSProcessInfo processInfo].operatingSystemVersionString,
      @"sdk_version" : @RQ_SDK_VERSION,
      @"compiler" : @RQ_CLANG_VERSION,
      @"upstream_sha" : @RQ_UPSTREAM_SHA,
      @"cases" : @(cases.count),
      @"limit" : @"Foundation on current macOS can differ from the iOS of the SDK era (NSURL parsing, NSDictionary order, NSString boolValue). Results are evidence for this Foundation only.",
      @"non_public_calls" : calls,
    };
    if (!rqWriteFile(@(argv[3]), [rqLine(meta) stringByAppendingString:@"\n"])) return 1;
  }
  return 0;
}
