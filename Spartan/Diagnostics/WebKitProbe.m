//
//  WebKitProbe.m
//  Argo
//

#import "WebKitProbe.h"
#import <dlfcn.h>
#import <objc/message.h>
#import <objc/runtime.h>

@interface ArgoWebKitProbeViewController ()
@property (nonatomic, strong) UITextView *logView;
@property (nonatomic, strong) UIView *webHost;
@property (nonatomic, strong) id webView;          // WKWebView, held as id on purpose
@property (nonatomic, assign) void *webkitHandle;
@property (nonatomic, assign) BOOL navigationFinished;
@end

@implementation ArgoWebKitProbeViewController

#pragma mark - Screen

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [UIColor blackColor];

    NSArray *titles = @[ @"1. 프레임워크 파일 확인",
                         @"2. dlopen",
                         @"3. 클래스 존재 확인",
                         @"4. WKWebView 만들기",
                         @"5. HTML 불러오기",
                         @"6. 자바스크립트 실행",
                         @"7. 화면에 실제로 그려지는가",
                         @"8. WebContent 프로세스 확인",
                         @"0. 로그 저장",
                         @"× 닫기" ];
    NSArray *sels = @[ @"doListFiles", @"doDlopen", @"doProbeClasses",
                       @"doCreateWebView", @"doLoadHTML", @"doEvalJS",
                       @"doSnapshot", @"doCheckWebContent", @"doSaveLog",
                       @"doClose" ];

    UIStackView *buttons = [[UIStackView alloc] init];
    buttons.axis = UILayoutConstraintAxisVertical;
    buttons.spacing = 8;
    buttons.translatesAutoresizingMaskIntoConstraints = NO;

    for (NSUInteger i = 0; i < titles.count; i++) {
        UIButton *b = [UIButton buttonWithType:UIButtonTypeSystem];
        [b setTitle:titles[i] forState:UIControlStateNormal];
        [b addTarget:self
              action:NSSelectorFromString(sels[i])
    forControlEvents:UIControlEventPrimaryActionTriggered];
        [buttons addArrangedSubview:b];
    }

    self.logView = [[UITextView alloc] init];
    self.logView.backgroundColor = [UIColor colorWithWhite:0.1 alpha:1.0];
    self.logView.textColor = [UIColor greenColor];
    self.logView.font = [UIFont fontWithName:@"Menlo" size:22];
    self.logView.userInteractionEnabled = NO;
    self.logView.translatesAutoresizingMaskIntoConstraints = NO;

    // The web view needs a real place in the hierarchy. An off-screen
    // WKWebView is allowed to skip compositing, which would make step 7
    // report a blank page that is blank for the wrong reason.
    self.webHost = [[UIView alloc] init];
    self.webHost.backgroundColor = [UIColor darkGrayColor];
    self.webHost.translatesAutoresizingMaskIntoConstraints = NO;

    [self.view addSubview:buttons];
    [self.view addSubview:self.logView];
    [self.view addSubview:self.webHost];

    UILayoutGuide *g = self.view.safeAreaLayoutGuide;
    [NSLayoutConstraint activateConstraints:@[
        [buttons.leadingAnchor constraintEqualToAnchor:g.leadingAnchor constant:40],
        [buttons.topAnchor constraintEqualToAnchor:g.topAnchor constant:30],
        [buttons.widthAnchor constraintEqualToConstant:460],

        [self.logView.leadingAnchor constraintEqualToAnchor:buttons.trailingAnchor constant:30],
        [self.logView.trailingAnchor constraintEqualToAnchor:g.trailingAnchor constant:-40],
        [self.logView.topAnchor constraintEqualToAnchor:g.topAnchor constant:30],
        [self.logView.heightAnchor constraintEqualToConstant:520],

        [self.webHost.leadingAnchor constraintEqualToAnchor:self.logView.leadingAnchor],
        [self.webHost.trailingAnchor constraintEqualToAnchor:self.logView.trailingAnchor],
        [self.webHost.topAnchor constraintEqualToAnchor:self.logView.bottomAnchor constant:20],
        [self.webHost.bottomAnchor constraintEqualToAnchor:g.bottomAnchor constant:-30],
    ]];

    UITapGestureRecognizer *menu = [[UITapGestureRecognizer alloc]
                                    initWithTarget:self action:@selector(doClose)];
    menu.allowedPressTypes = @[ @(UIPressTypeMenu) ];
    [self.view addGestureRecognizer:menu];

    [self log:@"WebKit 탐침. 1번부터 순서대로 누르십시오."];
    [self log:@"7번이 핵심입니다. 그 전까지는 모두 통과해도 의미가 없습니다."];
}

- (void)log:(NSString *)fmt, ... {
    va_list args;
    va_start(args, fmt);
    NSString *s = [[NSString alloc] initWithFormat:fmt arguments:args];
    va_end(args);
    NSLog(@"[ArgoWKProbe] %@", s);
    dispatch_async(dispatch_get_main_queue(), ^{
        self.logView.text = [self.logView.text stringByAppendingFormat:@"\n%@", s];
        if (self.logView.text.length > 0) {
            [self.logView scrollRangeToVisible:NSMakeRange(self.logView.text.length - 1, 1)];
        }
    });
}

#pragma mark - 1. What is actually on this device

- (void)doListFiles {
    [self log:@"\n--- 1. 프레임워크 파일 ---"];
    NSArray *dirs = @[ @"/System/Library/Frameworks",
                       @"/System/Library/PrivateFrameworks" ];
    NSFileManager *fm = [NSFileManager defaultManager];

    for (NSString *dir in dirs) {
        NSError *err = nil;
        NSArray *entries = [fm contentsOfDirectoryAtPath:dir error:&err];
        if (!entries) {
            [self log:@"%@ : 읽을 수 없음 (%@)", dir, err.localizedDescription];
            continue;
        }
        NSMutableArray *hits = [NSMutableArray array];
        for (NSString *e in entries) {
            if ([e hasPrefix:@"Web"] || [e hasPrefix:@"JavaScript"] || [e hasPrefix:@"Safari"]) {
                [hits addObject:e];
            }
        }
        [self log:@"%@ : 전체 %lu개, 웹 관련 %lu개",
            dir.lastPathComponent, (unsigned long)entries.count, (unsigned long)hits.count];
        for (NSString *h in [hits sortedArrayUsingSelector:@selector(compare:)]) {
            NSString *binary = [[dir stringByAppendingPathComponent:h]
                                stringByAppendingPathComponent:[h stringByDeletingPathExtension]];
            BOOL hasBinary = [fm fileExistsAtPath:binary];
            unsigned long long size = 0;
            if (hasBinary) {
                size = [[fm attributesOfItemAtPath:binary error:nil] fileSize];
            }
            [self log:@"   %@  실행파일:%@  %llu bytes",
                h, hasBinary ? @"있음" : @"없음", size];
        }
    }
    [self log:@"→ 실행파일이 0 bytes 면 공유캐시 안에 있는 것입니다. 정상입니다."];
}

#pragma mark - 2. dlopen

- (void)doDlopen {
    [self log:@"\n--- 2. dlopen ---"];
    NSArray *candidates = @[
        @"/System/Library/Frameworks/WebKit.framework/WebKit",
        @"/System/Library/PrivateFrameworks/WebKit.framework/WebKit",
        @"/System/Library/PrivateFrameworks/WebCore.framework/WebCore",
        @"/System/Library/PrivateFrameworks/WebKitLegacy.framework/WebKitLegacy",
        @"/System/Library/Frameworks/JavaScriptCore.framework/JavaScriptCore",
    ];
    for (NSString *path in candidates) {
        void *h = dlopen(path.UTF8String, RTLD_NOW);
        if (h) {
            [self log:@"OK   %@", path];
            if ([path containsString:@"WebKit.framework"] && !self.webkitHandle) {
                self.webkitHandle = h;
            }
        } else {
            const char *e = dlerror();
            [self log:@"실패 %@\n     %s", path, e ? e : "(이유 없음)"];
        }
    }
    [self log:self.webkitHandle ? @"→ WebKit 로드됨." : @"→ WebKit 을 못 열었습니다. 여기서 끝입니다."];
}

#pragma mark - 3. Classes

- (void)doProbeClasses {
    [self log:@"\n--- 3. 클래스 ---"];
    NSArray *names = @[ @"WKWebView", @"WKWebViewConfiguration", @"WKProcessPool",
                        @"WKUserContentController", @"WKPreferences", @"WKNavigation",
                        @"WKSnapshotConfiguration", @"WKWebsiteDataStore",
                        @"UIWebView", @"JSContext" ];
    for (NSString *n in names) {
        Class c = NSClassFromString(n);
        [self log:@"%@ %@", c ? @"있음" : @"없음", n];
    }

    Class wk = NSClassFromString(@"WKWebView");
    if (!wk) {
        [self log:@"→ WKWebView 가 없습니다. 프레임워크는 있어도 클래스가 빠진 것입니다."];
        return;
    }
    NSArray *sels = @[ @"initWithFrame:configuration:", @"loadHTMLString:baseURL:",
                       @"loadRequest:", @"evaluateJavaScript:completionHandler:",
                       @"takeSnapshotWithConfiguration:completionHandler:",
                       @"setNavigationDelegate:" ];
    for (NSString *s in sels) {
        SEL sel = NSSelectorFromString(s);
        BOOL ok = [wk instancesRespondToSelector:sel];
        [self log:@"%@ -%@", ok ? @"있음" : @"없음", s];
    }
}

#pragma mark - 4. Construct

- (void)doCreateWebView {
    [self log:@"\n--- 4. WKWebView 만들기 ---"];
    Class cfgClass = NSClassFromString(@"WKWebViewConfiguration");
    Class wkClass  = NSClassFromString(@"WKWebView");
    if (!cfgClass || !wkClass) {
        [self log:@"클래스가 없습니다. 3번을 먼저 하십시오."];
        return;
    }

    @try {
        id cfg = [[cfgClass alloc] init];
        [self log:@"설정 객체: %@", cfg ?: @"nil"];

        CGRect frame = self.webHost.bounds;
        if (CGRectIsEmpty(frame)) { frame = CGRectMake(0, 0, 1280, 360); }

        // objc_msgSend, because initWithFrame:configuration: takes a struct
        // and performSelector cannot pass one.
        id (*create)(id, SEL, CGRect, id) = (id (*)(id, SEL, CGRect, id))objc_msgSend;
        id web = create([wkClass alloc], @selector(initWithFrame:configuration:), frame, cfg);

        if (!web) { [self log:@"생성 실패 — nil 이 돌아왔습니다."]; return; }
        self.webView = web;
        [self log:@"생성됨: %@", [web class]];

        if ([web respondsToSelector:@selector(setNavigationDelegate:)]) {
            [web performSelector:@selector(setNavigationDelegate:) withObject:self];
            [self log:@"내비게이션 델리게이트 연결됨"];
        }

        UIView *v = (UIView *)web;
        v.frame = self.webHost.bounds;
        v.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
        [self.webHost addSubview:v];
        [self log:@"화면에 붙였습니다. 아래 회색 칸을 보십시오."];
    } @catch (NSException *ex) {
        [self log:@"예외: %@ — %@", ex.name, ex.reason];
    }
}

#pragma mark - 5. Load

- (void)doLoadHTML {
    [self log:@"\n--- 5. HTML 불러오기 ---"];
    if (!self.webView) { [self log:@"4번을 먼저 하십시오."]; return; }

    self.navigationFinished = NO;
    NSString *html = @"<html><head><meta charset='utf-8'>"
                      "<style>body{background:#1e6fd9;color:#fff;font-size:64px;"
                      "font-family:-apple-system,sans-serif;text-align:center;padding-top:60px}</style>"
                      "</head><body>한글 WebKit 렌더링 시험<br><b>ABC 123</b></body></html>";

    @try {
        [self.webView performSelector:@selector(loadHTMLString:baseURL:)
                           withObject:html
                           withObject:nil];
        [self log:@"요청 보냄. 델리게이트 응답을 기다립니다…"];
    } @catch (NSException *ex) {
        [self log:@"예외: %@ — %@", ex.name, ex.reason];
    }
}

// Delegate callbacks. Declared nowhere, matched by selector at runtime.
- (void)webView:(id)webView didFinishNavigation:(id)navigation {
    self.navigationFinished = YES;
    [self log:@"델리게이트: 불러오기 완료"];
}
- (void)webView:(id)webView didFailNavigation:(id)navigation withError:(NSError *)error {
    [self log:@"델리게이트: 실패 — %@", error.localizedDescription];
}
- (void)webView:(id)webView didFailProvisionalNavigation:(id)navigation withError:(NSError *)error {
    [self log:@"델리게이트: 시작 단계 실패 — %@", error.localizedDescription];
}
- (void)webViewWebContentProcessDidTerminate:(id)webView {
    [self log:@"델리게이트: WebContent 프로세스가 죽었습니다 ← 결정적 실패"];
}

#pragma mark - 6. JavaScript

- (void)doEvalJS {
    [self log:@"\n--- 6. 자바스크립트 ---"];
    if (!self.webView) { [self log:@"4번을 먼저 하십시오."]; return; }

    void (^handler)(id, NSError *) = ^(id result, NSError *error) {
        if (error) {
            [self log:@"JS 오류: %@", error.localizedDescription];
        } else {
            [self log:@"JS 결과: %@", result];
            [self log:@"→ 자바스크립트가 돕니다. JIT 없이 인터프리터로도 충분합니다."];
        }
    };

    @try {
        [self.webView performSelector:@selector(evaluateJavaScript:completionHandler:)
                           withObject:@"document.body.innerText + ' | ' + window.innerWidth"
                           withObject:handler];
    } @catch (NSException *ex) {
        [self log:@"예외: %@ — %@", ex.name, ex.reason];
    }
}

#pragma mark - 7. Pixels. The only step that settles anything.

- (void)doSnapshot {
    [self log:@"\n--- 7. 실제로 그려지는가 ---"];
    if (!self.webView) { [self log:@"4번을 먼저 하십시오."]; return; }
    if (!self.navigationFinished) { [self log:@"주의: 아직 불러오기 완료 신호가 없습니다."]; }

    if ([self.webView respondsToSelector:@selector(takeSnapshotWithConfiguration:completionHandler:)]) {
        void (^done)(UIImage *, NSError *) = ^(UIImage *image, NSError *error) {
            if (error || !image) {
                [self log:@"스냅샷 실패: %@", error.localizedDescription ?: @"이미지 없음"];
            } else {
                [self log:@"스냅샷 %gx%g", image.size.width, image.size.height];
                [self describePixelsOf:image label:@"스냅샷"];
            }
            [self snapshotLayer];
        };
        @try {
            [self.webView performSelector:@selector(takeSnapshotWithConfiguration:completionHandler:)
                               withObject:nil
                               withObject:done];
        } @catch (NSException *ex) {
            [self log:@"예외: %@ — %@", ex.name, ex.reason];
            [self snapshotLayer];
        }
    } else {
        [self log:@"takeSnapshot 이 없습니다. 레이어로 시도합니다."];
        [self snapshotLayer];
    }
}

- (void)snapshotLayer {
    UIView *v = (UIView *)self.webView;
    if (!v) { return; }
    UIGraphicsBeginImageContextWithOptions(v.bounds.size, NO, 1.0);
    [v.layer renderInContext:UIGraphicsGetCurrentContext()];
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    if (img) { [self describePixelsOf:img label:@"레이어"]; }
}

// A page that loaded but painted nothing comes back as one flat colour.
// Counting distinct colours separates "rendered" from "reported success".
- (void)describePixelsOf:(UIImage *)image label:(NSString *)label {
    CGImageRef cg = image.CGImage;
    if (!cg) { [self log:@"%@: CGImage 없음", label]; return; }

    size_t w = 64, h = 36;
    size_t bytesPerRow = w * 4;
    uint8_t *buf = calloc(h * bytesPerRow, 1);
    CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
    CGContextRef ctx = CGBitmapContextCreate(buf, w, h, 8, bytesPerRow, cs,
                                             kCGImageAlphaPremultipliedLast);
    CGContextDrawImage(ctx, CGRectMake(0, 0, w, h), cg);

    NSMutableSet *colours = [NSMutableSet set];
    double sum = 0;
    for (size_t i = 0; i < w * h; i++) {
        uint8_t r = buf[i*4], g = buf[i*4+1], b = buf[i*4+2];
        sum += (0.299*r + 0.587*g + 0.114*b) / 255.0;
        [colours addObject:@((r/16)*256 + (g/16)*16 + (b/16))];
    }
    double mean = sum / (double)(w * h);

    CGContextRelease(ctx);
    CGColorSpaceRelease(cs);
    free(buf);

    [self log:@"%@: 평균밝기 %.3f, 색 %lu가지", label, mean, (unsigned long)colours.count];
    if (colours.count <= 2) {
        [self log:@"   → 단색입니다. 불러오기는 됐어도 그려지지는 않았습니다."];
    } else {
        [self log:@"   → 여러 색이 나옵니다. ★ 실제로 그려졌습니다 ★"];
    }
}

#pragma mark - 8. The process that does the painting

- (void)doCheckWebContent {
    [self log:@"\n--- 8. WebContent 프로세스 ---"];
    NSArray *paths = @[ @"/System/Library/Frameworks/WebKit.framework/XPCServices",
                        @"/System/Library/PrivateFrameworks/WebKit.framework/XPCServices",
                        @"/usr/libexec",
                        @"/System/Library/LaunchDaemons" ];
    NSFileManager *fm = [NSFileManager defaultManager];
    BOOL foundAny = NO;
    for (NSString *p in paths) {
        NSArray *entries = [fm contentsOfDirectoryAtPath:p error:nil];
        if (!entries) { continue; }
        for (NSString *e in entries) {
            if ([e.lowercaseString containsString:@"webcontent"]
                || [e.lowercaseString containsString:@"webkit.networking"]
                || [e.lowercaseString containsString:@"webkit.gpu"]) {
                [self log:@"%@/%@", p.lastPathComponent, e];
                foundAny = YES;
            }
        }
    }
    if (!foundAny) {
        [self log:@"WebContent 서비스를 못 찾았습니다."];
        [self log:@"→ 7번이 단색이었다면 이것이 이유입니다. 그리는 일은 별도 프로세스가 합니다."];
    }
}

#pragma mark - 0. Keep the evidence

- (void)doSaveLog {
    NSDateFormatter *df = [[NSDateFormatter alloc] init];
    df.dateFormat = @"yyyyMMdd-HHmmss";
    NSString *name = [NSString stringWithFormat:@"argo-webkit-probe-%@.log",
                      [df stringFromDate:[NSDate date]]];

    NSArray *dests = @[ [@"/var/mobile/Media" stringByAppendingPathComponent:name],
                        [NSSearchPathForDirectoriesInDomains(NSDocumentDirectory,
                                                             NSUserDomainMask, YES).firstObject
                         stringByAppendingPathComponent:name] ];
    for (NSString *dest in dests) {
        NSError *err = nil;
        BOOL ok = [self.logView.text writeToFile:dest atomically:YES
                                        encoding:NSUTF8StringEncoding error:&err];
        if (ok) { [self log:@"\n저장됨: %@", dest]; return; }
        [self log:@"저장 실패 %@ — %@", dest, err.localizedDescription];
    }
}

- (void)doClose {
    // The probe loads a private framework into this process. Tearing the
    // web view down before leaving keeps a half-initialised WebContent
    // connection from following us back into the file browser.
    if (self.webView) {
        [(UIView *)self.webView removeFromSuperview];
        self.webView = nil;
    }
    if (self.onClose) {
        self.onClose();
    } else {
        [self dismissViewControllerAnimated:YES completion:nil];
    }
}

@end
