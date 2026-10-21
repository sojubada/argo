//
//  WebDocumentView.m
//  Argo
//

#import "WebDocumentView.h"
#import "WebDocumentShell.h"
#import <dlfcn.h>
#import <objc/message.h>

// The focused view is where a press arrives first. Handling it on the
// view controller meant it had already been spent moving focus.
@interface ArgoCanvasButton : UIButton
@property (nonatomic, copy) BOOL (^onPress)(UIPressType type);
@end

@implementation ArgoCanvasButton
- (void)pressesBegan:(NSSet<UIPress *> *)presses withEvent:(UIPressesEvent *)event {
    for (UIPress *press in presses) {
        if (self.onPress && self.onPress(press.type)) { return; }
    }
    [super pressesBegan:presses withEvent:event];
}
@end

@interface ArgoWebDocumentViewController ()
@property (nonatomic, strong) id webView;
@property (nonatomic, strong) UIView *webHost;
@property (nonatomic, strong) UIStackView *toolbar;
@property (nonatomic, strong) UILabel *titleLabel;
@property (nonatomic, strong) UILabel *statusLabel;
@property (nonatomic, strong) UIActivityIndicatorView *spinner;
@property (nonatomic, strong) ArgoCanvasButton *canvas;
@property (nonatomic, strong) UIButton *prevButton;
@property (nonatomic, strong) UIButton *nextButton;
@property (nonatomic, strong) UIButton *nightButton;
@property (nonatomic, strong) UIButton *paintButton;
@property (nonatomic, strong) NSString *stageDir;
@property (nonatomic, assign) BOOL isNight;
@property (nonatomic, assign) BOOL slidePainted;
@property (nonatomic, assign) BOOL toolbarVisible;
@property (nonatomic, strong) NSTimer *hideTimer;
@end

@implementation ArgoWebDocumentViewController

- (NSString *)ext { return self.documentPath.pathExtension.lowercaseString; }
- (BOOL)isDeck   { return [[self ext] isEqualToString:@"pptx"]; }
- (BOOL)isSheet  { NSString *e = [self ext];
                   return [e isEqualToString:@"xlsx"] || [e isEqualToString:@"xls"]; }

#pragma mark - Screen

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [UIColor colorWithWhite:0.07 alpha:1.0];

    self.webHost = [[UIView alloc] init];
    self.webHost.backgroundColor = [UIColor colorWithWhite:0.29 alpha:1.0];
    self.webHost.translatesAutoresizingMaskIntoConstraints = NO;
    [self.view addSubview:self.webHost];

    // One focusable thing over the page. The remote talks to the
    // document through it, because tvOS gives a web view no focus of
    // its own and there is nothing in a document to focus on anyway.
    self.canvas = [ArgoCanvasButton buttonWithType:UIButtonTypeCustom];
    self.canvas.translatesAutoresizingMaskIntoConstraints = NO;
    [self.canvas addTarget:self action:@selector(toggleToolbar)
          forControlEvents:UIControlEventPrimaryActionTriggered];
    __weak typeof(self) weakSelf = self;
    self.canvas.onPress = ^BOOL(UIPressType type) {
        return [weakSelf handlePress:type];
    };
    [self.view addSubview:self.canvas];

    [self buildToolbar];

    self.spinner = [[UIActivityIndicatorView alloc] init];
    self.spinner.translatesAutoresizingMaskIntoConstraints = NO;
    [self.spinner startAnimating];
    [self.view addSubview:self.spinner];

    UILayoutGuide *g = self.view.safeAreaLayoutGuide;
    [NSLayoutConstraint activateConstraints:@[
        [self.toolbar.leadingAnchor constraintEqualToAnchor:g.leadingAnchor constant:40],
        [self.toolbar.trailingAnchor constraintEqualToAnchor:g.trailingAnchor constant:-40],
        [self.toolbar.topAnchor constraintEqualToAnchor:g.topAnchor constant:8],

        [self.webHost.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
        [self.webHost.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
        [self.webHost.topAnchor constraintEqualToAnchor:self.view.topAnchor],
        [self.webHost.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],

        [self.canvas.leadingAnchor constraintEqualToAnchor:self.webHost.leadingAnchor],
        [self.canvas.trailingAnchor constraintEqualToAnchor:self.webHost.trailingAnchor],
        [self.canvas.topAnchor constraintEqualToAnchor:self.webHost.topAnchor],
        [self.canvas.bottomAnchor constraintEqualToAnchor:self.webHost.bottomAnchor],

        [self.spinner.centerXAnchor constraintEqualToAnchor:self.view.centerXAnchor],
        [self.spinner.centerYAnchor constraintEqualToAnchor:self.view.centerYAnchor],
    ]];

    [self.view bringSubviewToFront:self.toolbar];

    for (NSNumber *dir in @[ @(UISwipeGestureRecognizerDirectionUp),
                            @(UISwipeGestureRecognizerDirectionDown) ]) {
        UISwipeGestureRecognizer *sw = [[UISwipeGestureRecognizer alloc]
                                        initWithTarget:self action:@selector(didSwipe:)];
        sw.direction = (UISwipeGestureRecognizerDirection)dir.unsignedIntegerValue;
        [self.canvas addGestureRecognizer:sw];
    }

    UITapGestureRecognizer *menu = [[UITapGestureRecognizer alloc]
                                    initWithTarget:self action:@selector(closeOrHideToolbar)];
    menu.allowedPressTypes = @[ @(UIPressTypeMenu) ];
    [self.view addGestureRecognizer:menu];

    [self setToolbarVisible:YES animated:NO];
    [self prepareWebViewAndRender];
}

- (UIButton *)barButton:(NSString *)title action:(SEL)sel {
    UIButton *b = [UIButton buttonWithType:UIButtonTypeSystem];
    [b setTitle:title forState:UIControlStateNormal];
    b.titleLabel.font = [UIFont systemFontOfSize:26];
    [b addTarget:self action:sel forControlEvents:UIControlEventPrimaryActionTriggered];
    return b;
}

- (void)buildToolbar {
    self.titleLabel = [[UILabel alloc] init];
    self.titleLabel.text = self.documentPath.lastPathComponent ?: @"";
    self.titleLabel.font = [UIFont systemFontOfSize:26];
    self.titleLabel.textColor = [UIColor whiteColor];
    [self.titleLabel setContentCompressionResistancePriority:UILayoutPriorityDefaultLow
                                                     forAxis:UILayoutConstraintAxisHorizontal];

    self.statusLabel = [[UILabel alloc] init];
    self.statusLabel.font = [UIFont monospacedDigitSystemFontOfSize:26
                                                             weight:UIFontWeightRegular];
    self.statusLabel.textColor = [UIColor colorWithWhite:0.75 alpha:1.0];
    self.statusLabel.text = @"여는 중…";

    NSString *prevTitle = self.isSheet ? @"◀ 이전 시트" : @"◀ 이전";
    NSString *nextTitle = self.isSheet ? @"다음 시트 ▶" : @"다음 ▶";
    self.prevButton  = [self barButton:prevTitle action:@selector(goPrev)];
    self.nextButton  = [self barButton:nextTitle action:@selector(goNext)];
    self.nightButton = [self barButton:@"야간" action:@selector(toggleNight)];

    // Controls first. With the filename leading, a long one pushed the
    // navigation buttons off the right of the screen entirely.
    NSMutableArray *items = [NSMutableArray arrayWithObjects:
                             self.prevButton, self.nextButton,
                             self.statusLabel, self.nightButton, nil];
    if (self.isDeck) {
        // pptx-preview does not draw the slide master, so a deck whose
        // theme writes light text loses it against the unpainted page.
        self.paintButton = [self barButton:@"배경" action:@selector(togglePaint)];
        [items addObject:self.paintButton];
    }
    [items addObject:[self barButton:@"× 닫기" action:@selector(closeNow)]];
    self.titleLabel.lineBreakMode = NSLineBreakByTruncatingMiddle;
    [items addObject:self.titleLabel];

    self.toolbar = [[UIStackView alloc] initWithArrangedSubviews:items];
    self.toolbar.axis = UILayoutConstraintAxisHorizontal;
    self.toolbar.spacing = 26;
    self.toolbar.distribution = UIStackViewDistributionFill;
    self.toolbar.alignment = UIStackViewAlignmentCenter;
    self.toolbar.translatesAutoresizingMaskIntoConstraints = NO;
    [self.view addSubview:self.toolbar];
}

- (void)setToolbarVisible:(BOOL)visible animated:(BOOL)animated {
    self.toolbarVisible = visible;
    [self.hideTimer invalidate];
    // A disabled button cannot take focus, so with the toolbar away the
    // canvas is the only focusable thing and every arrow reaches it.
    for (UIView *sub in self.toolbar.arrangedSubviews) {
        if ([sub isKindOfClass:[UIControl class]]) {
            ((UIControl *)sub).enabled = visible;
        }
    }
    if (!visible) { [self setNeedsFocusUpdate]; [self updateFocusIfNeeded]; }
    void (^apply)(void) = ^{ self.toolbar.alpha = visible ? 1.0 : 0.0; };
    if (animated) { [UIView animateWithDuration:0.2 animations:apply]; } else { apply(); }
    if (visible) {
        self.hideTimer = [NSTimer scheduledTimerWithTimeInterval:8.0 repeats:NO
                                                         block:^(NSTimer *t) {
            [self setToolbarVisible:NO animated:YES];
        }];
    }
}

- (void)toggleToolbar { [self setToolbarVisible:!self.toolbarVisible animated:YES]; }

- (NSArray<id<UIFocusEnvironment>> *)preferredFocusEnvironments {
    return @[ self.canvas ];
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    [self setNeedsFocusUpdate];
    [self updateFocusIfNeeded];
}

#pragma mark - Remote

- (BOOL)handlePress:(UIPressType)type {
    // While the toolbar is up, Up is how focus reaches it. Everything
    // else keeps working, so paging does not need the toolbar dismissed.
    if (type == UIPressTypeUpArrow && self.toolbarVisible) { return NO; }
    switch (type) {
        case UIPressTypeUpArrow:    [self scrollBy:-420]; return YES;
        case UIPressTypeDownArrow:  [self scrollBy:420];  return YES;
        case UIPressTypeLeftArrow:  [self goPrev];        return YES;
        case UIPressTypeRightArrow: [self goNext];        return YES;
        case UIPressTypePlayPause:  [self toggleNight];   return YES;
        default: return NO;
    }
}

- (void)didSwipe:(UISwipeGestureRecognizer *)g {
    [self scrollBy:(g.direction == UISwipeGestureRecognizerDirectionUp) ? 420 : -420];
}

- (void)scrollBy:(int)dy {
    [self js:[NSString stringWithFormat:@"argoNav.scroll(%d);argoNav.label()", dy]
         then:^(id r) { [self showStatus:r]; }];
}
- (void)goNext { [self js:@"argoNav.next();argoNav.label()" then:^(id r){ [self showStatus:r]; }]; }
- (void)goPrev { [self js:@"argoNav.prev();argoNav.label()" then:^(id r){ [self showStatus:r]; }]; }

- (void)toggleNight {
    self.isNight = !self.isNight;
    [self.nightButton setTitle:self.isNight ? @"주간" : @"야간"
                      forState:UIControlStateNormal];
    self.webHost.backgroundColor = self.isNight
        ? [UIColor colorWithWhite:0.07 alpha:1.0]
        : [UIColor colorWithWhite:0.29 alpha:1.0];
    [self js:[NSString stringWithFormat:@"argoNav.night(%@)",
              self.isNight ? @"true" : @"false"] then:nil];
}

- (void)togglePaint {
    self.slidePainted = !self.slidePainted;
    [self js:[NSString stringWithFormat:@"argoNav.paint(%@)",
              self.slidePainted ? @"'#1f3864'" : @"''"] then:nil];
}

- (void)showStatus:(id)label {
    NSString *s = [NSString stringWithFormat:@"%@", label];
    if (s.length > 0 && ![s isEqualToString:@"<null>"]) { self.statusLabel.text = s; }
}

- (void)closeOrHideToolbar {
    if (self.toolbarVisible) { [self setToolbarVisible:NO animated:YES]; }
    else { [self closeNow]; }
}

- (void)closeNow {
    [self.hideTimer invalidate];
    if (self.webView) {
        [(UIView *)self.webView removeFromSuperview];
        self.webView = nil;
    }
    if (self.stageDir) {
        [[NSFileManager defaultManager] removeItemAtPath:self.stageDir error:nil];
        self.stageDir = nil;
    }
    if (self.onClose) { self.onClose(); }
    else { [self dismissViewControllerAnimated:YES completion:nil]; }
}

#pragma mark - Rendering

- (void)prepareWebViewAndRender {
    dlopen("/System/Library/Frameworks/WebKit.framework/WebKit", RTLD_NOW);
    Class cfgClass = NSClassFromString(@"WKWebViewConfiguration");
    Class wkClass  = NSClassFromString(@"WKWebView");
    if (!cfgClass || !wkClass) { [self showError:@"이 기기에서 WebKit을 찾지 못했습니다."]; return; }

    id cfg = [[cfgClass alloc] init];
    @try { [[cfg valueForKey:@"preferences"] setValue:@YES
                                               forKey:@"allowFileAccessFromFileURLs"]; }
    @catch (NSException *e) {}
    @try { [cfg setValue:@YES forKey:@"allowUniversalAccessFromFileURLs"]; }
    @catch (NSException *e) {}

    CGRect frame = self.view.bounds;
    id (*create)(id, SEL, CGRect, id) = (id (*)(id, SEL, CGRect, id))objc_msgSend;
    id web = create([wkClass alloc], @selector(initWithFrame:configuration:), frame, cfg);
    if (!web) { [self showError:@"웹뷰를 만들지 못했습니다."]; return; }

    self.webView = web;
    [web performSelector:@selector(setNavigationDelegate:) withObject:self];
    UIView *v = (UIView *)web;
    v.frame = self.webHost.bounds;
    v.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    v.userInteractionEnabled = NO;   // the canvas button owns the remote
    v.opaque = NO;
    [self.webHost addSubview:v];

    [self stageAndLoad];
}

- (void)stageAndLoad {
    NSString *ext = [self ext];
    NSArray *scripts = [ArgoWebDocumentShell scriptsForExtension:ext];
    if (scripts.count == 0) { [self showError:
        [NSString stringWithFormat:@".%@ 형식은 아직 지원하지 않습니다.", ext]]; return; }

    NSFileManager *fm = [NSFileManager defaultManager];
    NSString *dir = [NSTemporaryDirectory() stringByAppendingPathComponent:
                     [NSString stringWithFormat:@"argo_doc_%@", [[NSUUID UUID] UUIDString]]];
    [fm createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:nil];
    self.stageDir = dir;

    NSMutableString *tags = [NSMutableString string];
    for (NSString *n in scripts) {
        NSString *src = [[NSBundle mainBundle] pathForResource:n ofType:@"js" inDirectory:@"webdoc"];
        if (!src) { [self showError:[NSString stringWithFormat:@"%@.js 가 없습니다.", n]]; return; }
        [fm copyItemAtPath:src
                    toPath:[dir stringByAppendingPathComponent:[n stringByAppendingString:@".js"]]
                     error:nil];
        [tags appendFormat:@"<script src='%@.js'></script>", n];
    }

    // Read and encode off the main thread: a large workbook otherwise
    // freezes the screen before the spinner has a chance to turn.
    NSString *path = self.documentPath;
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_USER_INITIATED, 0), ^{
        NSData *doc = [NSData dataWithContentsOfFile:path];
        if (!doc) {
            dispatch_async(dispatch_get_main_queue(), ^{ [self showError:@"파일을 읽지 못했습니다."]; });
            return;
        }
        NSString *dataJS = [NSString stringWithFormat:@"window.__argoDoc=\"%@\";",
                            [doc base64EncodedStringWithOptions:0]];
        NSString *html = [ArgoWebDocumentShell pageForExtension:ext
                                                     scriptTags:tags
                                                       fileName:path.lastPathComponent];
        [dataJS writeToFile:[dir stringByAppendingPathComponent:@"data.js"]
                 atomically:YES encoding:NSUTF8StringEncoding error:nil];
        NSString *htmlPath = [dir stringByAppendingPathComponent:@"index.html"];
        [html writeToFile:htmlPath atomically:YES encoding:NSUTF8StringEncoding error:nil];

        dispatch_async(dispatch_get_main_queue(), ^{
            if (!self.webView) { return; }
            [self.webView performSelector:@selector(loadFileURL:allowingReadAccessToURL:)
                               withObject:[NSURL fileURLWithPath:htmlPath]
                               withObject:[NSURL fileURLWithPath:dir isDirectory:YES]];
        });
    });
}

- (void)webView:(id)webView didFinishNavigation:(id)navigation { [self pollReady:0]; }
- (void)webView:(id)webView didFailProvisionalNavigation:(id)nav withError:(NSError *)error {
    [self showError:error.localizedDescription];
}
- (void)webViewWebContentProcessDidTerminate:(id)webView {
    [self showError:@"문서가 너무 커서 표시하지 못했습니다."];
}

// renderAsync resolves after the navigation does, so the page is asked
// until it says it is finished rather than assumed to be.
- (void)pollReady:(int)attempt {
    if (attempt > 80) { [self showError:@"여는 데 너무 오래 걸립니다."]; return; }
    // A plain separator, because a universal character name may not
    // refer to a control character in an Objective-C literal.
    [self js:@"window.__argoState+'@@'+(window.__argoInfo||'')+'@@'+(window.__argoErr||'')"
         then:^(id result) {
        NSArray *parts = [[NSString stringWithFormat:@"%@", result]
                          componentsSeparatedByString:@"@@"];
        NSString *state = parts.firstObject ?: @"";
        if ([state isEqualToString:@"done"]) {
            [self.spinner stopAnimating];
            [self showStatus:parts.count > 1 ? parts[1] : @""];
            [self setToolbarVisible:YES animated:YES];
        } else if ([state isEqualToString:@"error"]) {
            [self showError:parts.count > 2 ? parts[2] : @"알 수 없는 오류"];
        } else {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.4 * NSEC_PER_SEC)),
                           dispatch_get_main_queue(), ^{ [self pollReady:attempt + 1]; });
        }
    }];
}

- (void)js:(NSString *)script then:(void (^)(id))then {
    if (!self.webView) { return; }
    void (^handler)(id, NSError *) = ^(id result, NSError *error) {
        if (!error && then) { then(result); }
    };
    @try {
        [self.webView performSelector:@selector(evaluateJavaScript:completionHandler:)
                           withObject:script withObject:handler];
    } @catch (NSException *ex) {}
}

- (void)showError:(NSString *)message {
    [self.spinner stopAnimating];
    self.statusLabel.text = @"열지 못했습니다";
    [self setToolbarVisible:YES animated:YES];

    UILabel *l = [[UILabel alloc] init];
    l.text = message;
    l.numberOfLines = 0;
    l.textAlignment = NSTextAlignmentCenter;
    l.font = [UIFont systemFontOfSize:28];
    l.textColor = [UIColor colorWithRed:1 green:0.6 blue:0.6 alpha:1];
    l.translatesAutoresizingMaskIntoConstraints = NO;
    [self.view addSubview:l];
    [NSLayoutConstraint activateConstraints:@[
        [l.centerXAnchor constraintEqualToAnchor:self.view.centerXAnchor],
        [l.centerYAnchor constraintEqualToAnchor:self.view.centerYAnchor],
        [l.widthAnchor constraintEqualToAnchor:self.view.widthAnchor multiplier:0.7],
    ]];
}

- (void)dealloc {
    [self.hideTimer invalidate];
    if (self.stageDir) {
        [[NSFileManager defaultManager] removeItemAtPath:self.stageDir error:nil];
    }
}

@end
