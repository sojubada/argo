//
//  WebDocProbe.h
//  Argo
//
//  The first probe settled that this device has a working WebKit.
//  This one asks the question that decides the architecture: can that
//  web view open a real document off the disk and render it the way
//  Word drew it — and can a remote control move around inside it.
//

#import <UIKit/UIKit.h>

@interface ArgoWebDocProbeViewController : UIViewController
@property (nonatomic, copy) void (^onClose)(void);
@end
