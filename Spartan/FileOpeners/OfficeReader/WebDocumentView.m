//
//  WebDocumentView.m
//  Argo
//

#import "WebDocumentView.h"
#import "ArgoTheme.h"
#import "WebDocumentShell.h"
#import <dlfcn.h>
#import <objc/message.h>
// Swift 쪽 진단 기록. 모듈 이름은 Argo 입니다.
#import "Argo-Swift.h"

// The focused view is where a press arrives first. Handling it on the
// view controller meant it had already been spent moving focus.
/// 문서 위에 올려 두는 유일한 포커스 대상.
///
/// **UIButton 이 아니라 UIView 입니다.** 전에는 버튼이었는데, tvOS 는
/// 포커스를 받은 버튼에 흰색 포커스 판을 깔아 줍니다. 이 뷰는 화면 전체를
/// 덮으므로 그 판이 문서를 통째로 가립니다 — PDF 뷰어가 흰 화면으로 나오던
/// 원인이고, 여기도 같은 구조라 같은 병을 갖고 있었습니다.
/// 포커스가 툴바에 있을 때만 문서가 보였던 것입니다.
@interface ArgoCanvasButton : UIControl
@property (nonatomic, copy) BOOL (^onPress)(UIPressType type);
@property (nonatomic, copy) void (^onSelect)(void);
@end

@implementation ArgoCanvasButton

/// 포커스를 받아야 입력이 옵니다. UIView 는 기본이 NO 입니다.
- (BOOL)canBecomeFocused { return YES; }

- (void)pressesBegan:(NSSet<UIPress *> *)presses withEvent:(UIPressesEvent *)event {
    for (UIPress *pr in presses) {
        [ArgoDiagLog log:@"canvas"
                 message:[NSString stringWithFormat:@"press type=%ld focused=%d",
                          (long)pr.type, (int)self.isFocused]];
    }
    for (UIPress *press in presses) {
        if (press.type == UIPressTypeSelect) {
            if (self.onSelect) { self.onSelect(); }
            return;
        }
        if (self.onPress && self.onPress(press.type)) { return; }
    }
    [super pressesBegan:presses withEvent:event];
}
@end

@interface ArgoWebDocumentViewController ()
@property (nonatomic, strong) id webView;
@property (nonatomic, strong) UIView *webHost;
@property (nonatomic, strong) UIStackView *toolbar;
@property (nonatomic, strong) ArgoSurfaceView *toolbarSurface;
@property (nonatomic, strong) UIView *loadingView;
@property (nonatomic, strong) UIView *errorView;
@property (nonatomic, strong) UIView *panelView;
@property (nonatomic, strong) UILabel *loadingTitle;
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
/// 보이는 것과 입력을 받는 것은 다릅니다. 열자마자 버튼이 포커스를
/// 가져가면 화살표가 버튼 사이만 오가고 쪽은 꼼짝도 하지 않습니다.
@property (nonatomic, assign) BOOL toolbarInteractive;
/// 0 원래 크기, 1 가로 꽉 참, 2 세로 꽉 참.
@property (nonatomic, assign) NSInteger fitIndex;
@property (nonatomic, strong) UIButton *fitButton;
@property (nonatomic, strong) NSTimer *hideTimer;
@end

@implementation ArgoWebDocumentViewController

- (NSString *)ext { return self.documentPath.pathExtension.lowercaseString; }
- (BOOL)isDeck   { return [[self ext] isEqualToString:@"pptx"]; }
- (BOOL)isSheet  { NSString *e = [self ext];
                   return [e isEqualToString:@"xlsx"] || [e isEqualToString:@"xls"]; }
- (BOOL)isPDF    { return [[self ext] isEqualToString:@"pdf"]; }

#pragma mark - Screen

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [ArgoTheme background];

    self.webHost = [[UIView alloc] init];
    self.webHost.backgroundColor = [ArgoTheme canvasBackground];
    self.webHost.translatesAutoresizingMaskIntoConstraints = NO;
    [self.view addSubview:self.webHost];

    // One focusable thing over the page. The remote talks to the
    // document through it, because tvOS gives a web view no focus of
    // its own and there is nothing in a document to focus on anyway.
    self.canvas = [[ArgoCanvasButton alloc] init];
    self.canvas.translatesAutoresizingMaskIntoConstraints = NO;
    self.canvas.backgroundColor = [UIColor clearColor];
    __weak typeof(self) weakSelf = self;
    self.canvas.onSelect = ^{ [weakSelf toggleToolbar]; };
    self.canvas.onPress = ^BOOL(UIPressType type) {
        return [weakSelf handlePress:type];
    };
    [self.view addSubview:self.canvas];

    [self buildToolbar];

    [self buildLoadingView];

    UILayoutGuide *g = self.view.safeAreaLayoutGuide;
    [NSLayoutConstraint activateConstraints:@[
        // 도구 모음은 아래에 둡니다. 위에 두면 문서의 첫 줄을 가립니다.
        [self.toolbarSurface.centerXAnchor constraintEqualToAnchor:g.centerXAnchor],
        [self.toolbarSurface.bottomAnchor constraintEqualToAnchor:g.bottomAnchor
                                                         constant:-[ArgoTheme gutter]],
        [self.toolbarSurface.leadingAnchor constraintGreaterThanOrEqualToAnchor:g.leadingAnchor
                                                                      constant:[ArgoTheme screenMargin]],
        [self.toolbarSurface.trailingAnchor constraintLessThanOrEqualToAnchor:g.trailingAnchor
                                                                     constant:-[ArgoTheme screenMargin]],

        [self.toolbar.leadingAnchor constraintEqualToAnchor:self.toolbarSurface.leadingAnchor
                                                  constant:[ArgoTheme gutter]],
        [self.toolbar.trailingAnchor constraintEqualToAnchor:self.toolbarSurface.trailingAnchor
                                                   constant:-[ArgoTheme gutter]],
        [self.toolbar.topAnchor constraintEqualToAnchor:self.toolbarSurface.topAnchor
                                              constant:[ArgoTheme gutter] * 0.5],
        [self.toolbar.bottomAnchor constraintEqualToAnchor:self.toolbarSurface.bottomAnchor
                                                 constant:-[ArgoTheme gutter] * 0.5],

        [self.webHost.leadingAnchor constraintEqualToAnchor:self.view.leadingAnchor],
        [self.webHost.trailingAnchor constraintEqualToAnchor:self.view.trailingAnchor],
        [self.webHost.topAnchor constraintEqualToAnchor:self.view.topAnchor],
        [self.webHost.bottomAnchor constraintEqualToAnchor:self.view.bottomAnchor],

        [self.canvas.leadingAnchor constraintEqualToAnchor:self.webHost.leadingAnchor],
        [self.canvas.trailingAnchor constraintEqualToAnchor:self.webHost.trailingAnchor],
        [self.canvas.topAnchor constraintEqualToAnchor:self.webHost.topAnchor],
        [self.canvas.bottomAnchor constraintEqualToAnchor:self.webHost.bottomAnchor],

        [self.loadingView.centerXAnchor constraintEqualToAnchor:self.view.centerXAnchor],
        [self.loadingView.centerYAnchor constraintEqualToAnchor:self.view.centerYAnchor],
        [self.loadingView.widthAnchor constraintEqualToConstant:760],
    ]];

    [self.view bringSubviewToFront:self.toolbarSurface];
    [self.view bringSubviewToFront:self.loadingView];

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

    [ArgoDiagLog begin:[NSString stringWithFormat:@"오피스 뷰어: %@",
                        self.documentPath.lastPathComponent]];
    [self setToolbarVisible:YES animated:NO];
    [self prepareWebViewAndRender];
}

/// 원래 크기 → 가로 꽉 참 → 세로 꽉 참 을 돕니다.
///
/// 쪽 하나의 실제 크기를 재서 배율을 겁니다. docx 는 폭이 pt 로 못 박혀
/// 있고 pptx 는 1180x664 로 그려지므로, 손대지 않으면 TV 에서 작게 뜹니다.
- (void)cycleFit {
    self.fitIndex = (self.fitIndex + 1) % 3;
    NSArray<NSString *> *modes  = @[ @"", @"w", @"h" ];
    NSArray<NSString *> *titles = @[ @"원래 크기", @"가로 꽉 참", @"세로 꽉 참" ];
    [self.fitButton setTitle:titles[self.fitIndex] forState:UIControlStateNormal];
    [ArgoDiagLog log:@"fit" message:titles[self.fitIndex]];
    [self js:[NSString stringWithFormat:@"argoNav.fit('%@');argoNav.label()",
              modes[self.fitIndex]]
         then:^(id label) { [self showStatus:label]; }];
    [self setToolbarVisible:YES interactive:YES animated:YES];
}

- (UIButton *)barButton:(NSString *)title action:(SEL)sel {
    ArgoBarButton *b = [ArgoBarButton barButtonWithTitle:title];
    [b addTarget:self action:sel forControlEvents:UIControlEventPrimaryActionTriggered];
    return b;
}

#pragma mark - 로딩 화면

/// 진행률을 측정할 수 없으므로 가짜 퍼센트를 보이지 않습니다.
/// 무엇을 여는 중인지만 말합니다.
- (void)buildLoadingView {
    ArgoSurfaceView *box = [ArgoSurfaceView makeSurface];

    self.spinner = [[UIActivityIndicatorView alloc] init];
    self.spinner.color = [ArgoTheme accent];
    self.spinner.translatesAutoresizingMaskIntoConstraints = NO;
    [self.spinner startAnimating];

    UILabel *what = [[UILabel alloc] init];
    what.text = @"문서를 여는 중";
    what.font = [ArgoTheme fontTitle];
    what.textColor = [ArgoTheme textPrimary];
    what.textAlignment = NSTextAlignmentCenter;
    what.translatesAutoresizingMaskIntoConstraints = NO;

    self.loadingTitle = [[UILabel alloc] init];
    self.loadingTitle.text = self.documentPath.lastPathComponent ?: @"";
    self.loadingTitle.font = [ArgoTheme fontCaption];
    self.loadingTitle.textColor = [ArgoTheme textSecondary];
    self.loadingTitle.textAlignment = NSTextAlignmentCenter;
    self.loadingTitle.lineBreakMode = NSLineBreakByTruncatingMiddle;
    self.loadingTitle.translatesAutoresizingMaskIntoConstraints = NO;

    [box addSubview:self.spinner];
    [box addSubview:what];
    [box addSubview:self.loadingTitle];

    CGFloat pad = [ArgoTheme screenMargin] * 0.7;
    [NSLayoutConstraint activateConstraints:@[
        [self.spinner.centerXAnchor constraintEqualToAnchor:box.centerXAnchor],
        [self.spinner.topAnchor constraintEqualToAnchor:box.topAnchor constant:pad],

        [what.topAnchor constraintEqualToAnchor:self.spinner.bottomAnchor constant:28],
        [what.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [what.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],

        [self.loadingTitle.topAnchor constraintEqualToAnchor:what.bottomAnchor constant:12],
        [self.loadingTitle.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [self.loadingTitle.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],
        [self.loadingTitle.bottomAnchor constraintEqualToAnchor:box.bottomAnchor constant:-pad],
    ]];

    self.loadingView = box;
    [self.view addSubview:box];
}

- (void)hideLoadingView {
    [self.spinner stopAnimating];
    [UIView animateWithDuration:0.2 animations:^{
        self.loadingView.alpha = 0;
    } completion:^(BOOL done) {
        [self.loadingView removeFromSuperview];
        self.loadingView = nil;
    }];
}

- (void)buildToolbar {
    // 파일명은 도구 모음에서 뺐습니다. 긴 이름 하나가 버튼을 화면 밖으로
    // 밀어냈고, 이름은 열 때 로딩 화면이 이미 보여줍니다.
    // 자세한 것은 '문서 정보' 패널에 있습니다.
    self.statusLabel = [[UILabel alloc] init];
    self.statusLabel.font = [ArgoTheme fontMonoBody];
    self.statusLabel.textColor = [ArgoTheme textSecondary];
    self.statusLabel.text = @"여는 중…";

    NSString *prevTitle = self.isSheet ? @"◀ 이전 시트" : @"◀ 이전";
    NSString *nextTitle = self.isSheet ? @"다음 시트 ▶" : @"다음 ▶";
    self.prevButton  = [self barButton:prevTitle action:@selector(goPrev)];
    self.nextButton  = [self barButton:nextTitle action:@selector(goNext)];

    NSMutableArray *items = [NSMutableArray arrayWithObjects:
                             self.prevButton, self.nextButton, self.statusLabel, nil];

    // 맞춤은 자주 쓰므로 설정 안에 묻지 않습니다. PDF 뷰어와 같은 자리입니다.
    self.fitButton = [self barButton:@"원래 크기" action:@selector(cycleFit)];
    [items addObject:self.fitButton];

    // 나머지는 전부 설정 하나로 모았습니다. 명세의 '버튼을 흩뿌리지 말 것'입니다.
    [items addObject:[self barButton:@"설정" action:@selector(showPanel)]];
    [items addObject:[self barButton:@"닫기" action:@selector(closeNow)]];

    self.toolbar = [[UIStackView alloc] initWithArrangedSubviews:items];
    self.toolbar.axis = UILayoutConstraintAxisHorizontal;
    self.toolbar.spacing = [ArgoTheme gutter];
    self.toolbar.distribution = UIStackViewDistributionFill;
    self.toolbar.alignment = UIStackViewAlignmentCenter;
    self.toolbar.translatesAutoresizingMaskIntoConstraints = NO;

    self.toolbarSurface = [ArgoSurfaceView makeSurface];
    [self.view addSubview:self.toolbarSurface];
    [self.toolbarSurface addSubview:self.toolbar];
}

- (void)setToolbarVisible:(BOOL)visible animated:(BOOL)animated {
    [self setToolbarVisible:visible interactive:NO animated:animated];
}

- (void)setToolbarVisible:(BOOL)visible
              interactive:(BOOL)interactive
                 animated:(BOOL)animated {
    BOOL wasInteractive = self.toolbarInteractive;
    [ArgoDiagLog log:@"toolbar"
             message:[NSString stringWithFormat:@"visible=%d interactive=%d",
                      (int)visible, (int)(visible && interactive)]];
    self.toolbarVisible = visible;
    self.toolbarInteractive = visible && interactive;
    [self.hideTimer invalidate];

    // 꺼진 버튼은 포커스를 못 받습니다. 이것이 캔버스가 화살표를 받는
    // 유일한 방법입니다.
    for (UIView *sub in self.toolbar.arrangedSubviews) {
        if ([sub isKindOfClass:[UIControl class]]) {
            ((UIControl *)sub).enabled = self.toolbarInteractive;
        }
    }
    void (^apply)(void) = ^{
        // 보이기만 할 때는 옅게 — 지금 눌리지 않는다는 뜻입니다.
        CGFloat a = visible ? (self.toolbarInteractive ? 1.0 : 0.55) : 0.0;
        self.toolbar.alpha = a;
        self.toolbarSurface.alpha = a;
    };
    // alpha 0 인 뷰는 포커스 후보에서 빠집니다. 들어갈 때는 투명도를
    // 먼저 맞춘 뒤 포커스를 요청해야 첫 select 가 도구 모음에 닿습니다.
    if (animated && !self.toolbarInteractive) {
        [UIView animateWithDuration:0.2 animations:apply];
    } else {
        apply();
    }

    // 이미 도구 모음 안에 있는데 또 요청하면 엔진이
    // preferredFocusEnvironments 를 다시 읽어 첫 버튼으로 되돌립니다.
    if (self.toolbarInteractive != wasInteractive) {
        [self setNeedsFocusUpdate];
        [self updateFocusIfNeeded];
    }

    if (visible) {
        NSTimeInterval wait = self.toolbarInteractive ? 12.0 : 5.0;
        self.hideTimer = [NSTimer scheduledTimerWithTimeInterval:wait repeats:NO
                                                           block:^(NSTimer *t) {
            [self setToolbarVisible:NO interactive:NO animated:YES];
        }];
    }
}

/// 선택 버튼. 도구 모음을 띄우고 포커스를 그쪽으로 넘깁니다.
- (void)toggleToolbar {
    [self setToolbarVisible:YES interactive:!self.toolbarInteractive animated:YES];
}

- (void)didUpdateFocusInContext:(UIFocusUpdateContext *)context
       withAnimationCoordinator:(UIFocusAnimationCoordinator *)coordinator {
    [super didUpdateFocusInContext:context withAnimationCoordinator:coordinator];
    [ArgoDiagLog log:@"focus"
             message:NSStringFromClass([context.nextFocusedItem class]) ?: @"nil"];
}

- (NSArray<id<UIFocusEnvironment>> *)preferredFocusEnvironments {
    [ArgoDiagLog log:@"focusAsk"
             message:[NSString stringWithFormat:@"interactive=%d", (int)self.toolbarInteractive]];
    // 패널이 열리면 포커스가 그 안에 머물러야 합니다. 뒤에 있는
    // 캔버스로 빠지면 방향키가 문서를 움직여 패널을 쓸 수 없습니다.
    if (self.errorView) { return @[ self.errorView ]; }
    if (self.panelView) { return @[ self.panelView ]; }
    if (self.toolbarInteractive) { return @[ self.toolbar ]; }
    return @[ self.canvas ];
}

#pragma mark - 설정 패널

/// 부가 기능을 하나로 모읍니다. 도구 모음에 버튼을 늘리지 않습니다.
- (void)showPanel {
    if (self.panelView) { return; }
    [self.hideTimer invalidate];

    ArgoSurfaceView *box = [ArgoSurfaceView makeSurface];

    UILabel *head = [[UILabel alloc] init];
    head.text = @"문서 설정";
    head.font = [ArgoTheme fontTitle];
    head.textColor = [ArgoTheme textPrimary];
    head.translatesAutoresizingMaskIntoConstraints = NO;

    NSMutableArray *rows = [NSMutableArray array];

    ArgoBarButton *night = [ArgoBarButton barButtonWithTitle:
                            self.isNight ? @"주간 보기로" : @"야간 보기로"];
    [night addTarget:self action:@selector(panelToggleNight)
    forControlEvents:UIControlEventPrimaryActionTriggered];
    [rows addObject:night];

    if (self.isDeck) {
        // pptx-preview는 슬라이드 마스터를 그리지 않아, 밝은 글자를 쓰는
        // 테마는 칠하지 않은 바탕에서 글자가 사라집니다.
        ArgoBarButton *paint = [ArgoBarButton barButtonWithTitle:@"슬라이드 배경 칠하기"];
        [paint addTarget:self action:@selector(panelTogglePaint)
        forControlEvents:UIControlEventPrimaryActionTriggered];
        [rows addObject:paint];
    }

    ArgoBarButton *info = [ArgoBarButton barButtonWithTitle:@"문서 정보"];
    [info addTarget:self action:@selector(panelShowInfo)
   forControlEvents:UIControlEventPrimaryActionTriggered];
    [rows addObject:info];

    ArgoBarButton *close = [ArgoBarButton barButtonWithTitle:@"닫기"];
    close.isPrimary = YES;
    [close addTarget:self action:@selector(hidePanel)
    forControlEvents:UIControlEventPrimaryActionTriggered];
    [rows addObject:close];

    UIStackView *stack = [[UIStackView alloc] initWithArrangedSubviews:rows];
    stack.axis = UILayoutConstraintAxisVertical;
    stack.spacing = 16;
    stack.alignment = UIStackViewAlignmentFill;
    stack.translatesAutoresizingMaskIntoConstraints = NO;

    [box addSubview:head];
    [box addSubview:stack];

    CGFloat pad = [ArgoTheme screenMargin] * 0.6;
    // 계층에 먼저 넣는다. self.view 쪽 앵커와 묶는 제약은
    // 공통 조상이 있어야 활성화된다 — 없으면 예외가 던져지고,
    // 잡는 곳이 없어 앱이 그대로 죽는다.
    [self.view addSubview:box];
    [NSLayoutConstraint activateConstraints:@[
        [head.topAnchor constraintEqualToAnchor:box.topAnchor constant:pad],
        [head.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [head.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],

        [stack.topAnchor constraintEqualToAnchor:head.bottomAnchor constant:28],
        [stack.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [stack.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],
        [stack.bottomAnchor constraintEqualToAnchor:box.bottomAnchor constant:-pad],

        // 문서가 완전히 사라지지 않도록 화면 일부만 가립니다.
        [box.trailingAnchor constraintEqualToAnchor:self.view.safeAreaLayoutGuide.trailingAnchor
                                           constant:-[ArgoTheme screenMargin]],
        [box.centerYAnchor constraintEqualToAnchor:self.view.centerYAnchor],
        [box.widthAnchor constraintEqualToConstant:620],
    ]];

    box.alpha = 0;
    self.panelView = box;
    [self.view bringSubviewToFront:box];
    [UIView animateWithDuration:0.2 animations:^{ box.alpha = 1; }];

    [self setNeedsFocusUpdate];
    [self updateFocusIfNeeded];
}

- (void)hidePanel {
    UIView *box = self.panelView;
    self.panelView = nil;
    [UIView animateWithDuration:0.2 animations:^{
        box.alpha = 0;
    } completion:^(BOOL done) {
        [box removeFromSuperview];
    }];
    // 패널을 닫으면 도구 모음으로 돌아갑니다.
    [self setToolbarVisible:YES animated:NO];
    [self setNeedsFocusUpdate];
    [self updateFocusIfNeeded];
}

- (void)panelToggleNight { [self toggleNight]; [self hidePanel]; }
- (void)panelTogglePaint { [self togglePaint]; [self hidePanel]; }

/// 파일명·형식·크기·수정 시각. 열람 화면을 가리지 않는 별도 패널입니다.
- (void)panelShowInfo {
    NSString *path = self.documentPath ?: @"";
    NSDictionary *attrs = [[NSFileManager defaultManager] attributesOfItemAtPath:path error:nil];

    NSByteCountFormatter *bytes = [[NSByteCountFormatter alloc] init];
    bytes.countStyle = NSByteCountFormatterCountStyleFile;
    NSString *size = attrs[NSFileSize]
        ? [bytes stringFromByteCount:[attrs[NSFileSize] longLongValue]] : @"알 수 없음";

    NSString *when = @"알 수 없음";
    if (attrs[NSFileModificationDate]) {
        NSDateFormatter *df = [[NSDateFormatter alloc] init];
        df.dateFormat = @"yyyy-MM-dd HH:mm";
        when = [df stringFromDate:attrs[NSFileModificationDate]];
    }

    NSString *body = [NSString stringWithFormat:
                      @"파일명\n%@\n\n형식\n%@\n\n크기\n%@\n\n수정\n%@\n\n상태\n%@",
                      path.lastPathComponent ?: @"",
                      [self ext].length > 0 ? [[self ext] uppercaseString] : @"없음",
                      size, when,
                      self.statusLabel.text ?: @""];

    [self hidePanel];

    ArgoSurfaceView *box = [ArgoSurfaceView makeSurface];

    UILabel *head = [[UILabel alloc] init];
    head.text = @"문서 정보";
    head.font = [ArgoTheme fontTitle];
    head.textColor = [ArgoTheme textPrimary];
    head.translatesAutoresizingMaskIntoConstraints = NO;

    UILabel *detail = [[UILabel alloc] init];
    detail.text = body;
    detail.numberOfLines = 0;
    detail.font = [ArgoTheme fontCaption];
    detail.textColor = [ArgoTheme textSecondary];
    detail.translatesAutoresizingMaskIntoConstraints = NO;

    ArgoBarButton *close = [ArgoBarButton barButtonWithTitle:@"닫기"];
    close.isPrimary = YES;
    [close addTarget:self action:@selector(hidePanel)
    forControlEvents:UIControlEventPrimaryActionTriggered];

    [box addSubview:head];
    [box addSubview:detail];
    [box addSubview:close];

    CGFloat pad = [ArgoTheme screenMargin] * 0.6;
    // 계층에 먼저 넣는다. self.view 쪽 앵커와 묶는 제약은
    // 공통 조상이 있어야 활성화된다 — 없으면 예외가 던져지고,
    // 잡는 곳이 없어 앱이 그대로 죽는다.
    [self.view addSubview:box];
    [NSLayoutConstraint activateConstraints:@[
        [head.topAnchor constraintEqualToAnchor:box.topAnchor constant:pad],
        [head.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],

        [detail.topAnchor constraintEqualToAnchor:head.bottomAnchor constant:24],
        [detail.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [detail.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],

        [close.topAnchor constraintEqualToAnchor:detail.bottomAnchor constant:32],
        [close.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [close.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],
        [close.bottomAnchor constraintEqualToAnchor:box.bottomAnchor constant:-pad],

        [box.trailingAnchor constraintEqualToAnchor:self.view.safeAreaLayoutGuide.trailingAnchor
                                           constant:-[ArgoTheme screenMargin]],
        [box.centerYAnchor constraintEqualToAnchor:self.view.centerYAnchor],
        [box.widthAnchor constraintEqualToConstant:680],
    ]];

    self.panelView = box;
    [self.view bringSubviewToFront:box];
    [self setNeedsFocusUpdate];
    [self updateFocusIfNeeded];
}

- (void)viewDidAppear:(BOOL)animated {
    [super viewDidAppear:animated];
    [self setNeedsFocusUpdate];
    [self updateFocusIfNeeded];
}

#pragma mark - Remote

- (BOOL)handlePress:(UIPressType)type {
    // 네 방향 모두 여기서 받습니다. 도구 모음은 선택 버튼으로만 갑니다 —
    // 도구 모음을 화면 아래로 옮기면서, 위쪽을 넘겨 줘도 갈 곳이 없어
    // 아무 일도 일어나지 않게 됐습니다. 쪽 넘김이 안 되던 이유입니다.
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
    self.webHost.backgroundColor = self.isNight
        ? [ArgoTheme background]
        : [ArgoTheme canvasBackground];
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
    // 메뉴 버튼은 한 겹씩 벗깁니다. 패널 -> 도구 모음 -> 닫기.
    if (self.panelView) { [self hidePanel]; return; }
    if (self.toolbarInteractive) {
        [self setToolbarVisible:NO interactive:NO animated:YES];
    } else {
        [self closeNow];
    }
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

    // PDF.js fetches its worker by URL rather than through a tag, so it
    // has to be beside the page without being loaded into it.
    if (self.isPDF) {
        NSString *worker = [[NSBundle mainBundle] pathForResource:@"pdf.worker.min"
                                                           ofType:@"js" inDirectory:@"webdoc"];
        if (worker) {
            [fm copyItemAtPath:worker
                        toPath:[dir stringByAppendingPathComponent:@"pdf.worker.min.js"]
                         error:nil];
        }
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
            [self hideLoadingView];
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
    [self hideLoadingView];
    self.statusLabel.text = @"열지 못했습니다";
    [self setToolbarVisible:YES animated:YES];
    if (self.errorView) { return; }

    ArgoSurfaceView *box = [ArgoSurfaceView makeSurface];

    UILabel *head = [[UILabel alloc] init];
    head.text = @"문서를 열 수 없습니다";
    head.font = [ArgoTheme fontTitle];
    head.textColor = [ArgoTheme textPrimary];
    head.textAlignment = NSTextAlignmentCenter;
    head.translatesAutoresizingMaskIntoConstraints = NO;

    // 원인을 그대로 보여줍니다. 렌더러가 내놓은 말이 거칠어도
    // '알 수 없는 오류'보다 쓸모 있습니다.
    UILabel *why = [[UILabel alloc] init];
    why.text = message.length > 0 ? message : @"원인을 알 수 없습니다.";
    why.numberOfLines = 0;
    why.font = [ArgoTheme fontCaption];
    why.textColor = [ArgoTheme textSecondary];
    why.textAlignment = NSTextAlignmentCenter;
    why.translatesAutoresizingMaskIntoConstraints = NO;

    UILabel *name = [[UILabel alloc] init];
    name.text = self.documentPath.lastPathComponent ?: @"";
    name.font = [ArgoTheme fontCaption];
    name.textColor = [ArgoTheme textDisabled];
    name.textAlignment = NSTextAlignmentCenter;
    name.lineBreakMode = NSLineBreakByTruncatingMiddle;
    name.translatesAutoresizingMaskIntoConstraints = NO;

    // 복구 경로를 줍니다. 막다른 화면으로 두지 않습니다.
    ArgoBarButton *retry = [ArgoBarButton barButtonWithTitle:@"다시 시도"];
    retry.isPrimary = YES;
    [retry addTarget:self action:@selector(retryRender)
    forControlEvents:UIControlEventPrimaryActionTriggered];

    ArgoBarButton *back = [ArgoBarButton barButtonWithTitle:@"파일 목록으로"];
    [back addTarget:self action:@selector(closeNow)
   forControlEvents:UIControlEventPrimaryActionTriggered];

    UIStackView *buttons = [[UIStackView alloc] initWithArrangedSubviews:@[ retry, back ]];
    buttons.axis = UILayoutConstraintAxisHorizontal;
    buttons.spacing = [ArgoTheme gutter];
    buttons.alignment = UIStackViewAlignmentCenter;
    buttons.translatesAutoresizingMaskIntoConstraints = NO;

    for (UIView *v in @[ head, why, name, buttons ]) { [box addSubview:v]; }

    CGFloat pad = [ArgoTheme screenMargin] * 0.7;
    // 계층에 먼저 넣는다. self.view 쪽 앵커와 묶는 제약은
    // 공통 조상이 있어야 활성화된다 — 없으면 예외가 던져지고,
    // 잡는 곳이 없어 앱이 그대로 죽는다.
    [self.view addSubview:box];
    [NSLayoutConstraint activateConstraints:@[
        [head.topAnchor constraintEqualToAnchor:box.topAnchor constant:pad],
        [head.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [head.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],

        [why.topAnchor constraintEqualToAnchor:head.bottomAnchor constant:20],
        [why.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [why.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],

        [name.topAnchor constraintEqualToAnchor:why.bottomAnchor constant:12],
        [name.leadingAnchor constraintEqualToAnchor:box.leadingAnchor constant:pad],
        [name.trailingAnchor constraintEqualToAnchor:box.trailingAnchor constant:-pad],

        [buttons.topAnchor constraintEqualToAnchor:name.bottomAnchor constant:36],
        [buttons.centerXAnchor constraintEqualToAnchor:box.centerXAnchor],
        [buttons.bottomAnchor constraintEqualToAnchor:box.bottomAnchor constant:-pad],

        [box.centerXAnchor constraintEqualToAnchor:self.view.centerXAnchor],
        [box.centerYAnchor constraintEqualToAnchor:self.view.centerYAnchor],
        [box.widthAnchor constraintEqualToConstant:900],
    ]];

    self.errorView = box;
    [self.view bringSubviewToFront:box];
    // 패널이 열리면 포커스가 패널 안으로 들어가야 합니다.
    [self setNeedsFocusUpdate];
    [self updateFocusIfNeeded];
}

- (void)retryRender {
    [self.errorView removeFromSuperview];
    self.errorView = nil;
    [self buildLoadingView];
    UILayoutGuide *g = self.view.safeAreaLayoutGuide;
    [NSLayoutConstraint activateConstraints:@[
        [self.loadingView.centerXAnchor constraintEqualToAnchor:g.centerXAnchor],
        [self.loadingView.centerYAnchor constraintEqualToAnchor:g.centerYAnchor],
        [self.loadingView.widthAnchor constraintEqualToConstant:760],
    ]];
    self.statusLabel.text = @"여는 중…";
    [self prepareWebViewAndRender];
}

- (void)dealloc {
    [self.hideTimer invalidate];
    if (self.stageDir) {
        [[NSFileManager defaultManager] removeItemAtPath:self.stageDir error:nil];
    }
}

@end
