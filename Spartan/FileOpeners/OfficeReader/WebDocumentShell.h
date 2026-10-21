//
//  WebDocumentShell.h
//  Argo
//
//  Builds the HTML page a document is rendered into, and the little
//  navigator the remote drives it with.
//

#import <Foundation/Foundation.h>

@interface ArgoWebDocumentShell : NSObject

/// Scripts this format needs, by basename, in load order. Empty if the
/// format is not supported.
+ (NSArray<NSString *> *)scriptsForExtension:(NSString *)ext;

/// The complete page. `scriptTags` are the <script> elements for the
/// renderers; the document itself arrives as window.__argoDoc.
+ (NSString *)pageForExtension:(NSString *)ext
                    scriptTags:(NSString *)scriptTags
                      fileName:(NSString *)fileName;

@end
