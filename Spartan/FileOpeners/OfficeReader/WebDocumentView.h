//
//  WebDocumentView.h
//  Argo
//
//  Word, Excel, PowerPoint and Hangul documents, rendered by the
//  JavaScript libraries in webdoc/ inside the WebKit that tvOS ships
//  but does not expose. No WebKit symbol is referenced at compile
//  time; everything goes through dlopen and the runtime.
//

#import <UIKit/UIKit.h>

@interface ArgoWebDocumentViewController : UIViewController
@property (nonatomic, copy) NSString *documentPath;
@property (nonatomic, copy) void (^onClose)(void);
@end
