//
//  WebKitProbe.h
//  Argo
//
//  The tvOS SDK ships no WebKit headers and no WebKit stub to link
//  against. The device may still carry the framework, exactly as it
//  carried Sharing.framework for the AirDrop work. So everything here
//  goes through dlopen and the Objective-C runtime: no WebKit symbol is
//  ever referenced at compile time, and nothing is linked.
//
//  The question this answers is not "does the folder exist" but "do
//  pixels come out". Those are different questions and only the second
//  one matters.
//

#import <UIKit/UIKit.h>

@interface ArgoWebKitProbeViewController : UIViewController
@property (nonatomic, copy) void (^onClose)(void);
@end
