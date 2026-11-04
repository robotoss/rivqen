// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors
//
// Rivqen stand-in for the UIKit umbrella header, for the macOS lab build only.
// The upstream cache file imports UIKit for one notification name. This header
// declares that name; runner/main.m defines it. Nothing else of UIKit exists here.
#import <Foundation/Foundation.h>

extern NSString *const UIApplicationDidReceiveMemoryWarningNotification;
