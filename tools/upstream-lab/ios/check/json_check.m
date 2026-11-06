// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Check of the JSON writer of the iOS function runner (runner/json.m). Foundation only: no
// upstream code, no UPSTREAM. Run with: sh tools/upstream-lab/ios/check.sh

#import <Foundation/Foundation.h>

#include <math.h>

#import "json.h"

static int failures = 0;

static void expect(BOOL ok, NSString *what) {
  if (!ok) {
    failures++;
    printf("FAIL %s\n", what.UTF8String);
  } else {
    printf("ok %s\n", what.UTF8String);
  }
}

static void expectText(NSString *got, NSString *want, NSString *what) {
  BOOL ok = [got isEqualToString:want];
  expect(ok, ok ? what : [NSString stringWithFormat:@"%@: got %@, want %@", what, got, want]);
}

// The line must parse as JSON (fragments allowed, for a single value).
static BOOL rqValidJson(NSString *line) {
  NSData *data = [line dataUsingEncoding:NSUTF8StringEncoding];
  return [NSJSONSerialization JSONObjectWithData:data options:NSJSONReadingFragmentsAllowed error:NULL] != nil;
}

static void writerNeverWritesBareNonFinite(void) {
  NSArray *values = @[ @(NAN), @(INFINITY), @(-INFINITY) ];
  NSArray *want = @[ @"\"nan\"", @"\"inf\"", @"\"-inf\"" ];
  for (NSUInteger i = 0; i < values.count; i++) {
    NSString *line = rqLine(values[i]);
    expectText(line, want[i], [NSString stringWithFormat:@"writer: %@ is a JSON string", want[i]]);
    expect(rqValidJson(line), [NSString stringWithFormat:@"writer: %@ line is valid JSON", want[i]]);
  }
  NSString *nested = rqLine(@{@"b" : @[ @1, @(NAN) ], @"a" : @(-INFINITY)});
  expectText(nested, @"{\"a\":\"-inf\",\"b\":[1,\"nan\"]}", @"writer: nested values, sorted keys");
  expect(rqValidJson(nested), @"writer: nested line is valid JSON");
}

static void writerKeepsFiniteNumbers(void) {
  expectText(rqLine(@(0.1)), @"0.10000000000000001", @"writer: double with 17 digits");
  expectText(rqLine(@(-0.0)), @"-0", @"writer: negative zero");
  expectText(rqLine(@(1e308)), @"1e+308", @"writer: large finite double");
  expectText(rqLine(@(DBL_MAX)), @"1.7976931348623157e+308", @"writer: largest double");
  expectText(rqLine(@(42)), @"42", @"writer: integer");
  expectText(rqLine(@(-7LL)), @"-7", @"writer: negative integer");
  expectText(rqLine(@YES), @"true", @"writer: boolean");
  expectText(rqLine([NSNull null]), @"null", @"writer: null");
  NSString *line = rqLine(@[ @(0.1), @(1e308), @(-0.0), @(DBL_MIN) ]);
  expect(rqValidJson(line), @"writer: finite doubles are valid JSON");
}

static void nonFiniteFindsThePointer(void) {
  NSString *token = nil;
  expect(rqNonFinite(@{@"diff" : @{@"{a}" : @"1"}}, @"/result", &token) == nil, @"finder: no number, no pointer");
  expect(rqNonFinite(@{@"x" : @[ @1, @2.5, @YES ]}, @"/result", &token) == nil, @"finder: finite numbers, no pointer");

  token = nil;
  NSString *at = rqNonFinite(@{@"diff" : @{@"{a}" : @(-INFINITY)}}, @"/result", &token);
  expectText(at ?: @"(nil)", @"/result/diff/{a}", @"finder: pointer of -inf");
  expectText(token ?: @"(nil)", @"-inf", @"finder: token -inf");

  token = nil;
  at = rqNonFinite(@{@"l" : @[ @0, @(NAN) ]}, @"/result", &token);
  expectText(at ?: @"(nil)", @"/result/l/1", @"finder: pointer into a list");
  expectText(token ?: @"(nil)", @"nan", @"finder: token nan");

  token = nil;
  at = rqNonFinite(@{@"a/b~c" : @(INFINITY)}, @"/result", &token);
  expectText(at ?: @"(nil)", @"/result/a~1b~0c", @"finder: RFC 6901 escapes");
  expectText(token ?: @"(nil)", @"inf", @"finder: token inf");

  RQOrdered *o = rqKeepOrder(@{@"k" : @(NAN)});
  at = rqNonFinite(@{@"data" : o}, @"/result", NULL);
  expectText(at ?: @"(nil)", @"/result/data/k", @"finder: ordered object, no token pointer");

  // Two values that are not finite: the first in write order (sorted keys) is reported.
  token = nil;
  at = rqNonFinite(@{@"b" : @(NAN), @"a" : @(INFINITY)}, @"/result", &token);
  expectText(at ?: @"(nil)", @"/result/a", @"finder: first in write order");
}

// The number that a case file can hold: Foundation reads -1e400 as -inf (and refuses 1e400).
static void parserGivesInfinityForMinus1e400(void) {
  NSData *data = [@"{\"v\":-1e400}" dataUsingEncoding:NSUTF8StringEncoding];
  NSDictionary *d = [NSJSONSerialization JSONObjectWithData:data options:0 error:NULL];
  if (d == nil) {
    // Then samples/cases.json (case S-NF-01) does not parse on this Foundation: the runner stops.
    expect(NO, @"parser: this Foundation refuses -1e400; change case S-NF-01 of samples/cases.json");
    return;
  }
  NSString *token = nil;
  NSString *at = rqNonFinite(d, @"/input", &token);
  expectText(at ?: @"(nil)", @"/input/v", @"parser: -1e400 is not finite");
  expect(rqValidJson(rqLine(d)), @"parser: the line of -1e400 is valid JSON");
}

int main(void) {
  @autoreleasepool {
    writerNeverWritesBareNonFinite();
    writerKeepsFiniteNumbers();
    nonFiniteFindsThePointer();
    parserGivesInfinityForMinus1e400();
  }
  if (failures > 0) {
    printf("%d checks failed\n", failures);
    return 1;
  }
  printf("all checks passed\n");
  return 0;
}
