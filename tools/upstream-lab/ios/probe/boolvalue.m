// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Foundation probe (WP-01 S1 T-05): -[NSString boolValue] of the `template_change` case values.
// The iOS SDK reads `template-change` with boolValue (docs/engineering/protocol/legacy-divergences.md
// D-29); the SDK method itself is private session code (`template_change` is unsupported), so this
// probe measures only the Foundation call. No upstream code. Output: one JSON line per case.
// Usage: sh tools/upstream-lab/ios/probe/run.sh [cases.json [out.jsonl]]
#import <Foundation/Foundation.h>

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc != 3) {
      fprintf(stderr, "usage: boolvalue <cases.json> <out.jsonl>\n");
      return 2;
    }
    NSData *data = [NSData dataWithContentsOfFile:@(argv[1])];
    NSArray *cases = data ? [NSJSONSerialization JSONObjectWithData:data options:0 error:nil] : nil;
    if (![cases isKindOfClass:[NSArray class]]) {
      fprintf(stderr, "boolvalue: the case file is not a JSON array\n");
      return 2;
    }
    NSMutableString *out = [NSMutableString string];
    for (NSDictionary *c in cases) {
      if (![c isKindOfClass:[NSDictionary class]] || ![c[@"fn"] isEqual:@"template_change"]) continue;
      id value = [c[@"input"] isKindOfClass:[NSDictionary class]] ? c[@"input"][@"value"] : nil;
      if (![value isKindOfClass:[NSString class]]) continue;
      NSDictionary *line = @{@"id" : c[@"id"], @"value" : value, @"bool_value" : @([(NSString *)value boolValue])};
      NSData *json = [NSJSONSerialization dataWithJSONObject:line options:NSJSONWritingSortedKeys error:nil];
      [out appendFormat:@"%@\n", [[NSString alloc] initWithData:json encoding:NSUTF8StringEncoding]];
    }
    if (![out writeToFile:@(argv[2]) atomically:YES encoding:NSUTF8StringEncoding error:nil]) {
      fprintf(stderr, "boolvalue: cannot write %s\n", argv[2]);
      return 1;
    }
  }
  return 0;
}
