//
//  WebDocProbe.m
//  Argo
//

#import "WebDocProbe.h"
#import <dlfcn.h>
#import <objc/message.h>

@interface ArgoWebDocProbeViewController ()
@property (nonatomic, strong) UITextView *logView;
@property (nonatomic, strong) UIView *webHost;
@property (nonatomic, strong) id webView;
@property (nonatomic, strong) NSString *foundDocument;
@property (nonatomic, strong) NSArray<NSString *> *foundDocuments;
@property (nonatomic, assign) NSUInteger documentIndex;
@property (nonatomic, strong) NSString *stageDir;
@property (nonatomic, assign) NSTimeInterval renderStart;
@end

@implementation ArgoWebDocProbeViewController

#pragma mark - Screen

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [UIColor blackColor];

    NSArray *titles = @[ @"1. 번들 자원 확인",
                         @"2. 문서 찾기 / 다음 문서",
                         @"3. 웹뷰 준비",
                         @"4. 문서 렌더링",
                         @"5. 쪽 정보 읽기",
                         @"6. ↓ 스크롤",
                         @"7. ↑ 스크롤",
                         @"8. 그려졌는지 확인",
                         @"9. DOM 구조 보기",
                         @"10. 배경 채우기 시험",
                         @"0. 로그 저장",
                         @"× 닫기" ];
    NSArray *sels = @[ @"doCheckBundle", @"doFindDocument", @"doPrepareWebView",
                       @"doRenderDocument", @"doPageInfo", @"doScrollDown",
                       @"doScrollUp", @"doVerifyPainted", @"doDumpStructure",
                       @"doFillBackground", @"doSaveLog", @"doClose" ];

    UIStackView *buttons = [[UIStackView alloc] init];
    buttons.axis = UILayoutConstraintAxisVertical;
    buttons.spacing = 6;
    buttons.translatesAutoresizingMaskIntoConstraints = NO;
    for (NSUInteger i = 0; i < titles.count; i++) {
        UIButton *b = [UIButton buttonWithType:UIButtonTypeSystem];
        [b setTitle:titles[i] forState:UIControlStateNormal];
        [b addTarget:self action:NSSelectorFromString(sels[i])
    forControlEvents:UIControlEventPrimaryActionTriggered];
        [buttons addArrangedSubview:b];
    }

    self.logView = [[UITextView alloc] init];
    self.logView.backgroundColor = [UIColor colorWithWhite:0.1 alpha:1.0];
    self.logView.textColor = [UIColor greenColor];
    self.logView.font = [UIFont fontWithName:@"Menlo" size:20];
    self.logView.userInteractionEnabled = NO;
    self.logView.translatesAutoresizingMaskIntoConstraints = NO;

    self.webHost = [[UIView alloc] init];
    self.webHost.backgroundColor = [UIColor darkGrayColor];
    self.webHost.clipsToBounds = YES;
    self.webHost.translatesAutoresizingMaskIntoConstraints = NO;

    [self.view addSubview:buttons];
    [self.view addSubview:self.logView];
    [self.view addSubview:self.webHost];

    UILayoutGuide *g = self.view.safeAreaLayoutGuide;
    [NSLayoutConstraint activateConstraints:@[
        [buttons.leadingAnchor constraintEqualToAnchor:g.leadingAnchor constant:40],
        [buttons.topAnchor constraintEqualToAnchor:g.topAnchor constant:20],
        [buttons.widthAnchor constraintEqualToConstant:380],

        [self.logView.leadingAnchor constraintEqualToAnchor:buttons.trailingAnchor constant:24],
        [self.logView.trailingAnchor constraintEqualToAnchor:g.trailingAnchor constant:-40],
        [self.logView.topAnchor constraintEqualToAnchor:g.topAnchor constant:20],
        [self.logView.heightAnchor constraintEqualToConstant:380],

        [self.webHost.leadingAnchor constraintEqualToAnchor:self.logView.leadingAnchor],
        [self.webHost.trailingAnchor constraintEqualToAnchor:self.logView.trailingAnchor],
        [self.webHost.topAnchor constraintEqualToAnchor:self.logView.bottomAnchor constant:16],
        [self.webHost.bottomAnchor constraintEqualToAnchor:g.bottomAnchor constant:-20],
    ]];

    UITapGestureRecognizer *menu = [[UITapGestureRecognizer alloc]
                                    initWithTarget:self action:@selector(doClose)];
    menu.allowedPressTypes = @[ @(UIPressTypeMenu) ];
    [self.view addGestureRecognizer:menu];

    [self log:@"문서 렌더링 탐침. 1번부터 순서대로."];
    [self log:@"4번에서 원본 그대로 나오면 길이 열린 것입니다."];
}

- (void)log:(NSString *)fmt, ... {
    va_list args; va_start(args, fmt);
    NSString *s = [[NSString alloc] initWithFormat:fmt arguments:args];
    va_end(args);
    NSLog(@"[ArgoWebDoc] %@", s);
    dispatch_async(dispatch_get_main_queue(), ^{
        self.logView.text = [self.logView.text stringByAppendingFormat:@"\n%@", s];
        if (self.logView.text.length > 0) {
            [self.logView scrollRangeToVisible:NSMakeRange(self.logView.text.length - 1, 1)];
        }
    });
}

#pragma mark - 1. Is the renderer in the app

- (NSString *)bundledScript:(NSString *)name {
    NSString *p = [[NSBundle mainBundle] pathForResource:name ofType:@"js" inDirectory:@"webdoc"];
    if (!p) { p = [[NSBundle mainBundle] pathForResource:name ofType:@"js"]; }
    return p;
}

- (void)doCheckBundle {
    [self log:@"\n--- 1. 번들 자원 ---"];
    NSFileManager *fm = [NSFileManager defaultManager];
    for (NSString *n in @[ @"jszip.min", @"docx-preview.min",
                           @"xlsx.min", @"pptx.min", @"hwp.min" ]) {
        NSString *p = [self bundledScript:n];
        if (p) {
            unsigned long long sz = [[fm attributesOfItemAtPath:p error:nil] fileSize];
            [self log:@"있음 %@.js  %llu bytes", n, sz];
        } else {
            [self log:@"없음 %@.js  ← 번들에 안 들어갔습니다", n];
        }
    }
}

#pragma mark - 2. Find something real to open

- (void)doFindDocument {
    // Pressing again steps to the next document rather than rescanning,
    // so one button covers "what is here" and "try that other one".
    if (self.foundDocuments.count > 0) {
        self.documentIndex = (self.documentIndex + 1) % self.foundDocuments.count;
        self.foundDocument = self.foundDocuments[self.documentIndex];
        [self log:@"\n→ 다음 문서 (%lu/%lu): %@",
            (unsigned long)(self.documentIndex + 1),
            (unsigned long)self.foundDocuments.count,
            self.foundDocument.lastPathComponent];
        return;
    }

    [self log:@"\n--- 2. 문서 찾기 ---"];
    NSArray *wanted = @[ @"docx", @"xlsx", @"pptx", @"hwp", @"hwpx" ];
    NSArray *roots = @[ @"/var/mobile/Media",
                        NSSearchPathForDirectoriesInDomains(NSDocumentDirectory,
                                                            NSUserDomainMask, YES).firstObject,
                        @"/var/mobile/Media/Downloads" ];
    NSFileManager *fm = [NSFileManager defaultManager];
    NSMutableArray *hits = [NSMutableArray array];

    for (NSString *root in roots) {
        if (!root) { continue; }
        NSDirectoryEnumerator *en = [fm enumeratorAtPath:root];
        NSString *sub;
        int scanned = 0;
        while ((sub = [en nextObject]) && scanned < 6000) {
            scanned++;
            if (en.level > 3) { [en skipDescendants]; continue; }
            if ([sub.lastPathComponent hasPrefix:@"."]) { continue; }
            if ([wanted containsObject:sub.pathExtension.lowercaseString]) {
                [hits addObject:[root stringByAppendingPathComponent:sub]];
                if (hits.count >= 20) { break; }
            }
        }
        if (hits.count >= 20) { break; }
    }

    if (hits.count == 0) {
        [self log:@"문서를 못 찾았습니다."];
        [self log:@"→ /var/mobile/Media 에 docx·xlsx·pptx·hwp 를 두십시오."];
        return;
    }

    // Group the log by format so it is obvious what is missing.
    for (NSString *ext in wanted) {
        NSArray *ofType = [hits filteredArrayUsingPredicate:
                           [NSPredicate predicateWithBlock:^BOOL(NSString *p, NSDictionary *b) {
            return [p.pathExtension.lowercaseString isEqualToString:ext];
        }]];
        if (ofType.count == 0) {
            [self log:@".%@ : 없음", ext];
            continue;
        }
        [self log:@".%@ : %lu개", ext, (unsigned long)ofType.count];
        for (NSString *h in ofType) {
            unsigned long long sz = [[fm attributesOfItemAtPath:h error:nil] fileSize];
            [self log:@"   %@  (%llu KB)", h.lastPathComponent, sz / 1024];
        }
    }

    self.foundDocuments = hits;
    self.documentIndex = 0;
    self.foundDocument = hits.firstObject;
    [self log:@"→ 1/%lu: %@", (unsigned long)hits.count, self.foundDocument.lastPathComponent];
    [self log:@"→ 이 버튼을 다시 누르면 다음 문서로 넘어갑니다."];
}

#pragma mark - 3. A web view that may read from disk

- (void)doPrepareWebView {
    [self log:@"\n--- 3. 웹뷰 준비 ---"];
    dlopen("/System/Library/Frameworks/WebKit.framework/WebKit", RTLD_NOW);

    Class cfgClass = NSClassFromString(@"WKWebViewConfiguration");
    Class wkClass  = NSClassFromString(@"WKWebView");
    if (!cfgClass || !wkClass) { [self log:@"WebKit 클래스가 없습니다."]; return; }

    id cfg = [[cfgClass alloc] init];

    // A file:// page may not XHR its siblings unless this is on. It is a
    // private preference, so it is set by name and checked afterwards
    // rather than trusted.
    id prefs = [cfg valueForKey:@"preferences"];
    for (NSString *key in @[ @"allowFileAccessFromFileURLs", @"_allowFileAccessFromFileURLs" ]) {
        @try {
            [prefs setValue:@YES forKey:key];
            [self log:@"설정됨 preferences.%@", key];
        } @catch (NSException *e) {
            [self log:@"없는 설정 %@", key];
        }
    }
    @try {
        [cfg setValue:@YES forKey:@"allowUniversalAccessFromFileURLs"];
        [self log:@"설정됨 configuration.allowUniversalAccessFromFileURLs"];
    } @catch (NSException *e) {
        [self log:@"없는 설정 allowUniversalAccessFromFileURLs"];
    }

    CGRect frame = self.webHost.bounds;
    if (CGRectIsEmpty(frame)) { frame = CGRectMake(0, 0, 1280, 400); }
    id (*create)(id, SEL, CGRect, id) = (id (*)(id, SEL, CGRect, id))objc_msgSend;
    id web = create([wkClass alloc], @selector(initWithFrame:configuration:), frame, cfg);
    if (!web) { [self log:@"생성 실패"]; return; }

    self.webView = web;
    [web performSelector:@selector(setNavigationDelegate:) withObject:self];

    UIView *v = (UIView *)web;
    v.frame = self.webHost.bounds;
    v.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
    [self.webHost addSubview:v];

    BOOL canLoadFile = [web respondsToSelector:
                        @selector(loadFileURL:allowingReadAccessToURL:)];
    [self log:@"loadFileURL:allowingReadAccessToURL: %@", canLoadFile ? @"있음" : @"없음"];
    [self log:@"웹뷰 준비됨."];
}

#pragma mark - 4. The question that matters

// Which scripts each format needs, in load order.
- (NSArray<NSString *> *)scriptsForExtension:(NSString *)ext {
    if ([ext isEqualToString:@"docx"]) { return @[ @"jszip.min", @"docx-preview.min" ]; }
    if ([ext isEqualToString:@"xlsx"]) { return @[ @"xlsx.min" ]; }
    if ([ext isEqualToString:@"pptx"]) { return @[ @"pptx.min" ]; }
    if ([ext isEqualToString:@"hwp"] || [ext isEqualToString:@"hwpx"]) { return @[ @"hwp.min" ]; }
    return @[];
}

// The page's body differs per format; everything around it does not.
- (NSString *)renderScriptForExtension:(NSString *)ext {
    if ([ext isEqualToString:@"docx"]) {
        return @"docx.renderAsync(blob,document.getElementById('c'),null,"
                "{className:'docx',inWrapper:true,breakPages:true,experimental:true,useBase64URL:true})"
                " .then(function(){done(document.querySelectorAll('section.docx').length+' 쪽');})"
                " .catch(fail);";
    }
    if ([ext isEqualToString:@"xlsx"]) {
        // SheetJS gives values and merges but not cell styling, so the
        // grid is drawn here rather than taken from the file.
        return @"var wb=XLSX.read(arr,{type:'array'});"
                "var out='';"
                "wb.SheetNames.forEach(function(name,i){"
                "  var ws=wb.Sheets[name];"
                "  out+=\"<h2>\"+(i+1)+\". \"+name+\"</h2>\";"
                "  out+=XLSX.utils.sheet_to_html(ws,{editable:false});"
                "});"
                "document.getElementById('c').innerHTML=out;"
                "done(wb.SheetNames.length+' 시트: '+wb.SheetNames.join(', '));";
    }
    if ([ext isEqualToString:@"pptx"]) {
        return @"var p=pptxPreview.init(document.getElementById('c'),{width:1100,height:619});"
                "Promise.resolve(p.preview(arr.buffer))"
                " .then(function(){done(document.querySelectorAll('.pptx-preview-slide,section,svg').length+' 요소');})"
                " .catch(fail);";
    }
    if ([ext isEqualToString:@"hwp"] || [ext isEqualToString:@"hwpx"]) {
        // HWPViewer's third argument defaults to {type:'binary'}, which
        // means a binary string to the CFB layer underneath — hand it a
        // Uint8Array and it calls .split("") on it. Say 'array'.
        // Wrapped in a function because a bare `return` at the top level
        // of a script is a syntax error — which silently killed the whole
        // block last time, so nothing ran and nothing reported.
        return @"(function(){"
                " var el=document.getElementById('c');"
                " try{ new hwpjs.Viewer(el,arr,{type:'array'}); }"
                " catch(e1){"
                "  try{ new hwpjs.Viewer(el,window.__argoDoc,{type:'base64'}); }"
                "  catch(e2){ fail(e2); return; }"
                " }"
                " done(el.querySelectorAll('*').length+' 요소');"
                "})();";
    }
    return @"fail('지원하지 않는 형식입니다');";
}

- (void)doRenderDocument {
    [self log:@"\n--- 4. 문서 렌더링 ---"];
    if (!self.webView) { [self log:@"3번을 먼저."]; return; }
    if (!self.foundDocument) { [self log:@"2번을 먼저."]; return; }

    NSString *ext = self.foundDocument.pathExtension.lowercaseString;
    NSArray *scripts = [self scriptsForExtension:ext];
    if (scripts.count == 0) { [self log:@".%@ 는 아직 지원하지 않습니다.", ext]; return; }
    [self log:@"형식 .%@ — %@", ext, [scripts componentsJoinedByString:@", "]];

    NSFileManager *fm = [NSFileManager defaultManager];
    if (self.stageDir) { [fm removeItemAtPath:self.stageDir error:nil]; }
    NSString *dir = [NSTemporaryDirectory() stringByAppendingPathComponent:
                     [NSString stringWithFormat:@"argo_webdoc_%@", [[NSUUID UUID] UUIDString]]];
    [fm createDirectoryAtPath:dir withIntermediateDirectories:YES attributes:nil error:nil];
    self.stageDir = dir;

    NSMutableString *tags = [NSMutableString string];
    for (NSString *n in scripts) {
        NSString *src = [self bundledScript:n];
        if (!src) { [self log:@"번들에 %@.js 가 없습니다.", n]; return; }
        [fm copyItemAtPath:src
                    toPath:[dir stringByAppendingPathComponent:[n stringByAppendingString:@".js"]]
                     error:nil];
        [tags appendFormat:@"<script src='%@.js'></script>", n];
    }

    NSData *doc = [NSData dataWithContentsOfFile:self.foundDocument];
    if (!doc) { [self log:@"문서를 읽지 못했습니다."]; return; }
    [self log:@"문서 %lu KB 를 base64 로 싣습니다…", (unsigned long)(doc.length / 1024)];
    if (doc.length > 10 * 1024 * 1024) {
        [self log:@"⚠ 10MB가 넘습니다. base64 로 %lu MB 가 되어 느리거나 메모리가 모자랄 수 있습니다.",
            (unsigned long)(doc.length * 4 / 3 / 1024 / 1024)];
        [self log:@"  작은 파일로도 한 번 해보십시오 (2번을 눌러 다음 문서로)."];
    }

    NSString *dataJS = [NSString stringWithFormat:@"window.__argoDoc=\"%@\";",
                        [doc base64EncodedStringWithOptions:0]];
    [dataJS writeToFile:[dir stringByAppendingPathComponent:@"data.js"]
             atomically:YES encoding:NSUTF8StringEncoding error:nil];

    NSString *html = [NSString stringWithFormat:
    @"<!doctype html><html><head><meta charset='utf-8'>"
    "<style>"
    // The renderers inherit from their container, so the debug chrome's
    // own colours must not reach it: white text on a white slide is
    // indistinguishable from a slide that never rendered.
    "html,body{margin:0;padding:0;background:#555;font:20px -apple-system,sans-serif;}"
    "#c{color:#000;position:relative;min-height:200px;}"
    ".docx-wrapper{background:#555;padding:20px 0;}"
    ".docx-wrapper>section.docx{box-shadow:0 0 12px rgba(0,0,0,.6);margin-bottom:20px;}"
    "h2{color:#8cf;margin:16px 12px 6px;font-size:22px;}"
    "table{border-collapse:collapse;background:#fff;color:#000;margin:0 12px 24px;}"
    "td,th{border:1px solid #999;padding:4px 10px;font-size:17px;white-space:nowrap;}"
    "#err{color:#f66;padding:20px;white-space:pre-wrap;}"
    "</style></head><body>"
    "<div id='err'></div><div id='c'></div>"
    "%@"
    "<script src='data.js'></script>"
    "<script>"
    "window.__argoState='starting';window.__argoInfo='';"
    "function done(info){window.__argoInfo=info||'';window.__argoState='done';}"
    "function fail(e){window.__argoState='error';"
    " var m=e?((e.name||'Error')+': '+(e.message||String(e))):String(e);"
    " window.__argoErr=m;"
    " document.getElementById('err').textContent=m+'\\n\\n'+((e&&e.stack)||'');}"
    "try{"
    " var raw=atob(window.__argoDoc);"
    " var arr=new Uint8Array(raw.length);"
    " for(var i=0;i<raw.length;i++){arr[i]=raw.charCodeAt(i);}"
    " var blob=new Blob([arr]);"
    " %@"
    "}catch(e){fail(e);}"
    "</script></body></html>", tags, [self renderScriptForExtension:ext]];

    NSString *htmlPath = [dir stringByAppendingPathComponent:@"shell.html"];
    [html writeToFile:htmlPath atomically:YES encoding:NSUTF8StringEncoding error:nil];

    self.renderStart = [NSDate timeIntervalSinceReferenceDate];
    [self.webView performSelector:@selector(loadFileURL:allowingReadAccessToURL:)
                       withObject:[NSURL fileURLWithPath:htmlPath]
                       withObject:[NSURL fileURLWithPath:dir isDirectory:YES]];
    [self log:@"loadFileURL 보냄. 그려지기까지 몇 초 걸립니다…"];
}

- (void)webView:(id)webView didFinishNavigation:(id)navigation {
    NSTimeInterval took = [NSDate timeIntervalSinceReferenceDate] - self.renderStart;
    [self log:@"불러오기 완료 (%.2f초). 렌더링 상태를 묻습니다…", took];
    [self pollState:0];
}
- (void)webView:(id)webView didFailProvisionalNavigation:(id)nav withError:(NSError *)error {
    [self log:@"불러오기 실패: %@", error.localizedDescription];
}
- (void)webViewWebContentProcessDidTerminate:(id)webView {
    [self log:@"WebContent 프로세스가 죽었습니다 — 문서가 너무 크거나 메모리 부족입니다."];
}

// renderAsync is a promise, so the page is not finished when the
// navigation is. Ask it until it says so, and give up rather than hang.
- (void)pollState:(int)attempt {
    if (attempt > 60) { [self log:@"45초가 지나도 끝나지 않았습니다."]; return; }
    [self js:@"window.__argoState + '|' + window.__argoInfo" then:^(id result) {
        NSString *s = [NSString stringWithFormat:@"%@", result];
        if ([s hasPrefix:@"done"]) {
            NSTimeInterval took = [NSDate timeIntervalSinceReferenceDate] - self.renderStart;
            NSArray *parts = [s componentsSeparatedByString:@"|"];
            [self log:@"★ 렌더링 완료 — %.2f초, %@", took,
                parts.count > 1 ? parts[1] : @""];
            [self log:@"→ 아래 칸을 보십시오. 원본 서식 그대로여야 합니다."];
        } else if ([s hasPrefix:@"error"]) {
            [self js:@"window.__argoErr || ''" then:^(id msg) {
                [self log:@"렌더링 오류: %@", msg];
                [self js:@"(document.getElementById('err').textContent||'').split('\\n').slice(2,6).join('\\n')"
                     then:^(id st) { [self log:@"%@", st]; }];
            }];
        } else {
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.75 * NSEC_PER_SEC)),
                           dispatch_get_main_queue(), ^{
                [self pollState:attempt + 1];
            });
        }
    }];
}

- (void)js:(NSString *)script then:(void (^)(id result))then {
    if (!self.webView) { return; }
    void (^handler)(id, NSError *) = ^(id result, NSError *error) {
        if (error) { [self log:@"JS 오류: %@", error.localizedDescription]; }
        else if (then) { then(result); }
    };
    @try {
        [self.webView performSelector:@selector(evaluateJavaScript:completionHandler:)
                           withObject:script withObject:handler];
    } @catch (NSException *ex) {
        [self log:@"예외: %@", ex.reason];
    }
}

#pragma mark - 5~7. Moving around with a remote

- (void)doPageInfo {
    [self log:@"\n--- 5. 쪽 정보 ---"];
    [self js:@"[document.body.scrollHeight, window.innerHeight,"
           " document.querySelectorAll('section.docx').length,"
           " document.querySelectorAll('img').length,"
           " document.querySelectorAll('table').length,"
           " document.querySelectorAll('tr').length,"
           " document.querySelectorAll('svg,canvas').length].join(' / ')"
        then:^(id r) {
        [self log:@"전체높이 / 화면높이 / 쪽 / 그림 / 표 / 행 / 도형"];
        [self log:@"%@", r];
    }];

    // When the pane comes back blank, these say whether nothing was
    // built, or it was built somewhere off-screen, or it was built in
    // a colour that cannot be seen.
    [self js:@"(function(){var c=document.getElementById('c');"
           " var r=c.getBoundingClientRect();"
           " var k=c.firstElementChild;"
           " var kr=k?k.getBoundingClientRect():{width:0,height:0,top:0,left:0};"
           " var cs=k?getComputedStyle(k):null;"
           " return '컨테이너 '+Math.round(r.width)+'x'+Math.round(r.height)"
           "  +' | 첫자식 <'+(k?k.tagName.toLowerCase():'없음')+'> '"
           "  +Math.round(kr.width)+'x'+Math.round(kr.height)"
           "  +' @'+Math.round(kr.left)+','+Math.round(kr.top)"
           "  +' | 글자색 '+(cs?cs.color:'-')+' 배경 '+(cs?cs.backgroundColor:'-')"
           "  +' | 자식수 '+c.children.length;})()"
        then:^(id r) {
        [self log:@"%@", r];
    }];

    [self js:@"(document.getElementById('c').innerText||'').replace(/\\s+/g,' ').slice(0,140)"
        then:^(id r) {
        NSString *txt = [NSString stringWithFormat:@"%@", r];
        [self log:@"글자: %@", txt.length > 0 ? txt : @"(없음 — 도형만 있거나 비었습니다)"];
    }];
}

- (void)doScrollDown { [self scrollBy:600]; }
- (void)doScrollUp   { [self scrollBy:-600]; }

- (void)scrollBy:(int)delta {
    NSString *s = [NSString stringWithFormat:
                   @"window.scrollBy(0,%d); Math.round(window.scrollY) + ' / ' + "
                   "Math.round(document.body.scrollHeight - window.innerHeight)", delta];
    [self js:s then:^(id r) {
        [self log:@"스크롤 %@", r];
    }];
}

#pragma mark - 8. Pixels again

- (void)doVerifyPainted {
    [self log:@"\n--- 8. 그려졌는지 ---"];
    UIView *v = (UIView *)self.webView;
    if (!v) { [self log:@"3번을 먼저."]; return; }
    UIGraphicsBeginImageContextWithOptions(v.bounds.size, NO, 1.0);
    [v.layer renderInContext:UIGraphicsGetCurrentContext()];
    UIImage *img = UIGraphicsGetImageFromCurrentImageContext();
    UIGraphicsEndImageContext();
    if (!img.CGImage) { [self log:@"이미지를 못 얻었습니다."]; return; }

    size_t w = 80, h = 45, bpr = w * 4;
    uint8_t *buf = calloc(h * bpr, 1);
    CGColorSpaceRef cs = CGColorSpaceCreateDeviceRGB();
    CGContextRef ctx = CGBitmapContextCreate(buf, w, h, 8, bpr, cs,
                                             kCGImageAlphaPremultipliedLast);
    CGContextDrawImage(ctx, CGRectMake(0, 0, w, h), img.CGImage);
    NSMutableSet *colours = [NSMutableSet set];
    double sum = 0;
    for (size_t i = 0; i < w * h; i++) {
        uint8_t r = buf[i*4], g = buf[i*4+1], b = buf[i*4+2];
        sum += (0.299*r + 0.587*g + 0.114*b) / 255.0;
        [colours addObject:@((r/16)*256 + (g/16)*16 + (b/16))];
    }
    CGContextRelease(ctx); CGColorSpaceRelease(cs); free(buf);
    [self log:@"평균밝기 %.3f, 색 %lu가지", sum / (double)(w*h), (unsigned long)colours.count];
    [self log:colours.count <= 2 ? @"→ 단색. 그려지지 않았습니다."
                                 : @"→ ★ 문서가 그려졌습니다 ★"];
}

#pragma mark - 9. The shape of what was built

// A pane that is blank while the DOM is full of shapes and text means
// the content is somewhere, in some colour, clipped by something. Only
// the tree says which.
- (void)doDumpStructure {
    [self log:@"\n--- 9. DOM 구조 ---"];
    [self js:@"(function(){var out=[];"
           " function walk(el,d){"
           "  if(d>3||out.length>13)return;"
           "  var r=el.getBoundingClientRect(),cs=getComputedStyle(el);"
           "  var cls=(typeof el.className==='string'&&el.className)?"
           "    '.'+el.className.trim().split(/\\s+/).slice(0,2).join('.'):'';"
           "  out.push('  '.repeat(d)+'<'+el.tagName.toLowerCase()+cls+'> '"
           "   +Math.round(r.width)+'x'+Math.round(r.height)"
           "   +' @'+Math.round(r.left)+','+Math.round(r.top)"
           "   +' ov:'+cs.overflow+' pos:'+cs.position"
           "   +' n='+el.children.length);"
           "  for(var i=0;i<Math.min(el.children.length,3);i++)walk(el.children[i],d+1);"
           " }"
           " walk(document.getElementById('c'),0);"
           " return out.join('\\n');})()"
        then:^(id r) { [self log:@"%@", r]; }];

    [self js:@"(function(){"
           " var all=document.querySelectorAll('#c *');"
           " for(var i=0;i<all.length;i++){"
           "  var e=all[i];"
           "  if(e.children.length===0&&(e.textContent||'').trim().length>1){"
           "   var cs=getComputedStyle(e),r=e.getBoundingClientRect();"
           "   return '첫 글자요소 <'+e.tagName.toLowerCase()+'> \"'"
           "    +(e.textContent||'').trim().slice(0,24)+'\" '"
           "    +Math.round(r.width)+'x'+Math.round(r.height)"
           "    +' @'+Math.round(r.left)+','+Math.round(r.top)"
           "    +' | '+cs.color+' '+cs.fontSize"
           "    +' vis:'+cs.visibility+' op:'+cs.opacity+' disp:'+cs.display;"
           "  }"
           " }"
           " return '글자를 가진 요소가 없습니다 (도형으로만 그려졌습니다)';})()"
        then:^(id r) { [self log:@"%@", r]; }];
}

#pragma mark - 10. Is it white text on a white slide

// The deck's first text span is rgb(255,255,255) and slide-master-wrapper
// has no children: the master, which carries the dark background this
// theme's white text was written for, did not render. Painting the
// slide dark is not a fix, but it settles whether that is the reason.
- (void)doFillBackground {
    [self log:@"\n--- 10. 배경 채우기 ---"];
    [self js:@"(function(){"
           " var imgs=document.querySelectorAll('#c img');"
           " var broken=0,src='';"
           " for(var i=0;i<imgs.length;i++){"
           "  if(!imgs[i].naturalWidth)broken++;"
           "  if(!src&&imgs[i].src)src=imgs[i].src.slice(0,28);"
           " }"
           " return '그림 '+imgs.length+'개 중 '+broken+'개가 안 떴습니다 | src: '+(src||'-');})()"
        then:^(id r) { [self log:@"%@", r]; }];

    [self js:@"(function(){"
           " var st=document.createElement('style');"
           " st.textContent='.slide-background{background:#1f3864 !important;}';"
           " document.head.appendChild(st);"
           " return '슬라이드 배경을 남색으로 칠했습니다.';})()"
        then:^(id r) {
        [self log:@"%@", r];
        [self log:@"→ 이제 글자가 보이면, 원인은 마스터 배경이 안 그려진 것입니다."];
        [self log:@"→ 그래도 백지면 글자가 다른 곳에 있습니다."];
    }];
}

#pragma mark - 0

- (void)doSaveLog {
    NSDateFormatter *df = [[NSDateFormatter alloc] init];
    df.dateFormat = @"yyyyMMdd-HHmmss";
    NSString *name = [NSString stringWithFormat:@"argo-webdoc-probe-%@.log",
                      [df stringFromDate:[NSDate date]]];
    for (NSString *dest in @[ [@"/var/mobile/Media" stringByAppendingPathComponent:name],
                              [NSSearchPathForDirectoriesInDomains(NSDocumentDirectory,
                                NSUserDomainMask, YES).firstObject
                               stringByAppendingPathComponent:name] ]) {
        if ([self.logView.text writeToFile:dest atomically:YES
                                  encoding:NSUTF8StringEncoding error:nil]) {
            [self log:@"\n저장됨: %@", dest];
            return;
        }
    }
    [self log:@"저장 실패"];
}

- (void)doClose {
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

@end
