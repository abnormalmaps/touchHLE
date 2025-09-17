/*
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */
// Objective C headers we don't have open-source equivalents for.
#ifndef TOUCHHLE_OBJC_SYSTEM_H
#define TOUCHHLE_OBJC_SYSTEM_H
// Objective-C base:
typedef signed char BOOL;
#define false 0
#define true 1
typedef struct objc_selector *SEL;
typedef struct objc_class *Class;
typedef struct objc_object {
  Class isa;
} *id;
id objc_msgSend(id, SEL, ...);
@interface NSObject {
  Class isa;
}
+ (id)new;
- (id)init;
@end
#endif // TOUCHHLE_OBJC_SYSTEM_H
