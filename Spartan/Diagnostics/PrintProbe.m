//
//  PrintProbe.m
//  Argo
//

#import "PrintProbe.h"
#import <objc/runtime.h>
#import <objc/message.h>

@interface ArgoPrintProbeViewController ()
@property (nonatomic, strong) UITextView *logView;
@property (nonatomic, strong) NSMutableString *logText;
@property (nonatomic, strong) NSData *lastPDF;
@end

@implementation ArgoPrintProbeViewController

static const CGFloat kPageWidth  = 595.2;   // A4, 72dpi
static const CGFloat kPageHeight = 841.8;
static const CGFloat kPageMargin = 20.0;

- (void)viewDidLoad {
    [super viewDidLoad];
    self.view.backgroundColor = [UIColor blackColor];

    self.logText = [NSMutableString string];
    self.logView = [[UITextView alloc] initWithFrame:CGRectInset(self.view.bounds, 60, 60)];
    self.logView.backgroundColor = [UIColor blackColor];
    self.logView.textColor = [UIColor greenColor];
    self.logView.font = [UIFont fontWithName:@"Menlo" size:22] ?: [UIFont systemFontOfSize:22];
    self.logView.userInteractionEnabled = YES;
    [self.view addSubview:self.logView];

    [self log:@"=== 인쇄 파이프라인 탐침 ==="];
    [self log:@"헤더는 API_UNAVAILABLE(tvos) 로 막혀 있습니다."];
    [self log:@"기기에 실제로 있는지를 런타임으로 묻습니다."];
    [self log:@""];

    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.3 * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
        [self runAll];
    });
}

- (void)pressesBegan:(NSSet<UIPress *> *)presses withEvent:(UIPressesEvent *)event {
    for (UIPress *press in presses) {
        if (press.type == UIPressTypeMenu) {
            if (self.onClose) { self.onClose(); }
            return;
        }
    }
    [super pressesBegan:presses withEvent:event];
}

- (void)log:(NSString *)line {
    [self.logText appendFormat:@"%@\n", line];
    self.logView.text = self.logText;
    NSUInteger len = self.logText.length;
    if (len > 0) {
        [self.logView scrollRangeToVisible:NSMakeRange(len - 1, 1)];
    }
    NSLog(@"[PrintProbe] %@", line);
}

- (void)runAll {
    [self step1ClassesInBinary];

    // 2~5단계는 더 이상 돌리지 않는다.
    //
    // 실기기에서 SIGSEGV 로 앱이 죽었다 (주소가 어떤 영역에도 없음).
    // 클래스는 바이너리에 있지만, 호출하면 tvOS 에서 초기화되지 않은
    // 인쇄 기반을 건드려 UIKit 안에서 터진다. 잡을 수 있는 예외가 아니라
    // 메모리 접근 위반이라 @try 로도 못 막는다.
    //
    // 질문에는 답이 나왔다. '막혀 있다'가 '컴파일러만 거부한다'가 아니라
    // '기기에서도 못 쓴다'였다. 더 캐물을 것이 없으므로 호출을 멈춘다.
    [self log:@""];
    [self log:@"[2-5] 돌리지 않습니다"];
    [self log:@"    이 호출들은 실기기에서 SIGSEGV 로 앱을 죽였습니다."];
    [self log:@"    클래스는 있지만 tvOS 에서 실제로 동작하지 않습니다."];
    [self log:@"    -> 3단계는 CoreText 로 직접 쪽을 나누는 길로 갑니다."];
    [self log:@""];
    [self log:@"=== 끝. 메뉴 버튼으로 닫습니다 ==="];
}

/// 아래 호출들은 기기를 죽인다. 기록을 위해 남겨 두되 부르지 않는다.
- (void)unusedDangerousSteps {
    [self step2SimpleText];
    [self step3Markup];
    [self step4ReadBack];
    [self step5Korean];
}

#pragma mark - 1. 클래스가 기기의 UIKit 바이너리에 있는가

- (void)step1ClassesInBinary {
    [self log:@"[1] 런타임 클래스 조회"];
    [self log:@"    헤더에서 막힌 것이 바이너리에도 없다는 뜻은 아닙니다."];

    NSArray *names = @[@"UIPrintPageRenderer",
                       @"UIPrintFormatter",
                       @"UISimpleTextPrintFormatter",
                       @"UIMarkupTextPrintFormatter",
                       @"UIGraphicsPDFRenderer"];
    for (NSString *name in names) {
        Class cls = NSClassFromString(name);
        [self log:[NSString stringWithFormat:@"    %@ %@",
                   cls ? @"있음  " : @"없음  <-- 못 씁니다", name]];
    }

    BOOL responds = [UIView instancesRespondToSelector:NSSelectorFromString(@"viewPrintFormatter")];
    [self log:[NSString stringWithFormat:@"    %@ UIView.viewPrintFormatter",
               responds ? @"응답함" : @"응답 안 함  <-- 못 씁니다"]];
    [self log:@""];
}

#pragma mark - 2. 글자가 여러 쪽으로 나뉘는가

- (void)step2SimpleText {
    [self log:@"[2] 글자 300줄 -> 쪽 나누기"];

    Class fmtClass = NSClassFromString(@"UISimpleTextPrintFormatter");
    if (!fmtClass) {
        [self log:@"    건너뜀 — UISimpleTextPrintFormatter 가 없습니다"];
        [self log:@""];
        return;
    }

    NSMutableString *body = [NSMutableString string];
    for (int i = 1; i <= 300; i++) {
        [body appendFormat:@"%d번째 줄입니다. The quick brown fox jumps over the lazy dog. 가나다라마바사.\n", i];
    }
    NSDictionary *attrs = @{ NSFontAttributeName: [UIFont systemFontOfSize:12],
                             NSForegroundColorAttributeName: [UIColor blackColor] };
    NSAttributedString *attributed = [[NSAttributedString alloc] initWithString:body attributes:attrs];

    id formatter = ((id (*)(id, SEL, id))objc_msgSend)
        ([fmtClass alloc], NSSelectorFromString(@"initWithAttributedText:"), attributed);
    if (!formatter) {
        [self log:@"    실패 — 서식기를 만들 수 없습니다"];
        [self log:@""];
        return;
    }

    NSInteger pages = 0;
    NSData *data = [self paginate:formatter pageCount:&pages];
    if (!data) {
        [self log:@"    실패 — 쪽이 안 나왔습니다"];
        [self log:@""];
        return;
    }
    [self log:[NSString stringWithFormat:@"    쪽 수: %ld", (long)pages]];
    [self log:[NSString stringWithFormat:@"    PDF 크기: %lu 바이트", (unsigned long)data.length]];
    if (pages <= 1) {
        [self log:@"    주의 — 300줄이 한 쪽이면 쪽 나누기가 안 되는 것입니다"];
    }
    self.lastPDF = data;
    [self log:@""];
}

#pragma mark - 3. HTML 이 쪽으로 나뉘는가 (핵심)

- (void)step3Markup {
    [self log:@"[3] HTML 표 120행 -> 쪽 나누기  ★핵심★"];
    [self log:@"    되면 WebKit 없이 HTML 을 쪽으로 나눌 수 있습니다."];
    [self log:@"    지금 JS 렌더러가 이미 HTML 을 만들고 있습니다."];

    Class fmtClass = NSClassFromString(@"UIMarkupTextPrintFormatter");
    if (!fmtClass) {
        [self log:@"    건너뜀 — UIMarkupTextPrintFormatter 가 없습니다"];
        [self log:@"    -> CoreText 로 직접 쪽을 나누는 길로 가야 합니다"];
        [self log:@""];
        return;
    }

    NSMutableString *rows = [NSMutableString string];
    for (int i = 1; i <= 120; i++) {
        [rows appendFormat:@"<tr><td>%d</td><td>항목 %d</td><td>설명 가나다 %d</td><td>%d</td></tr>",
         i, i, i, i * 1234];
    }
    NSString *html = [NSString stringWithFormat:
        @"<html><head><meta charset=\"utf-8\"><style>"
         "body{font-family:-apple-system,sans-serif;font-size:11pt;color:#000}"
         "h1{font-size:18pt}table{border-collapse:collapse;width:100%%}"
         "td,th{border:1px solid #888;padding:4px 6px}"
         "</style></head><body><h1>표 쪽 나누기 시험</h1>"
         "<p>표가 여러 쪽에 걸쳐 나뉘는지, 테두리와 한글이 살아 있는지 봅니다.</p>"
         "<table><tr><th>번호</th><th>이름</th><th>설명</th><th>값</th></tr>%@</table>"
         "</body></html>", rows];

    id formatter = ((id (*)(id, SEL, id))objc_msgSend)
        ([fmtClass alloc], NSSelectorFromString(@"initWithMarkupText:"), html);
    if (!formatter) {
        [self log:@"    실패 — 서식기를 만들 수 없습니다"];
        [self log:@""];
        return;
    }

    NSInteger pages = 0;
    NSData *data = [self paginate:formatter pageCount:&pages];
    if (!data) {
        [self log:@"    실패 — 쪽이 안 나왔습니다. HTML 경로는 못 씁니다."];
        [self log:@""];
        return;
    }
    [self log:[NSString stringWithFormat:@"    쪽 수: %ld", (long)pages]];
    [self log:[NSString stringWithFormat:@"    PDF 크기: %lu 바이트", (unsigned long)data.length]];
    if (pages <= 1) {
        [self log:@"    주의 — 120행이 한 쪽이면 표 쪽 나누기가 안 되는 것입니다"];
    }
    self.lastPDF = data;

    NSString *out = @"/var/mobile/Media/argo-print-probe.pdf";
    if ([data writeToFile:out atomically:YES]) {
        [self log:[NSString stringWithFormat:@"    저장: %@", out]];
        [self log:@"    (탐색기에서 열어 눈으로 확인하십시오)"];
    }
    [self log:@""];
}

#pragma mark - 4. CGPDFDocument 로 되읽어 잉크를 센다

- (void)step4ReadBack {
    [self log:@"[4] CGPDFDocument 되읽기 + 잉크 비율"];
    [self log:@"    PDFKit 이 없으니 빈 쪽 판정은 CGPDF 로 해야 합니다."];

    if (!self.lastPDF) {
        [self log:@"    건너뜀 — 읽을 PDF 가 없습니다"];
        [self log:@""];
        return;
    }

    CGDataProviderRef provider = CGDataProviderCreateWithCFData((CFDataRef)self.lastPDF);
    if (!provider) {
        [self log:@"    실패 — 데이터 제공자를 못 만들었습니다"];
        [self log:@""];
        return;
    }
    CGPDFDocumentRef doc = CGPDFDocumentCreateWithProvider(provider);
    CGDataProviderRelease(provider);
    if (!doc) {
        [self log:@"    실패 — CGPDFDocument 가 PDF 를 못 읽었습니다"];
        [self log:@""];
        return;
    }

    size_t count = CGPDFDocumentGetNumberOfPages(doc);
    [self log:[NSString stringWithFormat:@"    CGPDF 쪽 수: %zu", count]];

    size_t limit = count < 3 ? count : 3;
    for (size_t i = 1; i <= limit; i++) {
        CGPDFPageRef page = CGPDFDocumentGetPage(doc, i);
        if (!page) { continue; }
        CGFloat ratio = [self inkRatioOfPage:page];
        [self log:[NSString stringWithFormat:@"    %zu쪽: 잉크 %.4f  -> %@",
                   i, ratio, ratio < 0.002 ? @"빈 쪽" : @"내용 있음"]];
    }
    CGPDFDocumentRelease(doc);
    [self log:@""];
}

/// 쪽을 작게 흑백으로 그려 어두운 점의 비율을 센다.
/// PDFEasy 의 pageInkRatio 와 같은 생각이지만 PDFPage.thumbnail 대신
/// CGContextDrawPDFPage 를 쓴다 — PDFKit 이 없기 때문이다.
- (CGFloat)inkRatioOfPage:(CGPDFPageRef)page {
    CGRect box = CGPDFPageGetBoxRect(page, kCGPDFCropBox);
    if (box.size.width <= 0 || box.size.height <= 0) { return 0; }

    CGFloat longSide = box.size.width > box.size.height ? box.size.width : box.size.height;
    CGFloat scale = 100.0 / longSide;
    size_t w = (size_t)MAX(1, lround(box.size.width * scale));
    size_t h = (size_t)MAX(1, lround(box.size.height * scale));

    unsigned char *pixels = calloc(w * h, 1);
    if (!pixels) { return 0; }
    memset(pixels, 255, w * h);

    CGColorSpaceRef gray = CGColorSpaceCreateDeviceGray();
    CGContextRef ctx = CGBitmapContextCreate(pixels, w, h, 8, w, gray, kCGImageAlphaNone);
    CGColorSpaceRelease(gray);
    if (!ctx) { free(pixels); return 0; }

    // 흰색을 먼저 깐다. 투명한 쪽이 검은 쪽으로 읽히면 판정이 거꾸로 된다.
    CGContextSetGrayFillColor(ctx, 1.0, 1.0);
    CGContextFillRect(ctx, CGRectMake(0, 0, w, h));

    CGContextScaleCTM(ctx, scale, scale);
    CGContextTranslateCTM(ctx, -box.origin.x, -box.origin.y);
    CGContextDrawPDFPage(ctx, page);
    CGContextRelease(ctx);

    size_t inked = 0;
    for (size_t i = 0; i < w * h; i++) {
        if (pixels[i] < 250) { inked++; }
    }
    free(pixels);
    return (CGFloat)inked / (CGFloat)(w * h);
}

#pragma mark - 5. 한글이 그려지는가

- (void)step5Korean {
    [self log:@"[5] 한글 글꼴 확인"];

    Class fmtClass = NSClassFromString(@"UIMarkupTextPrintFormatter");
    if (!fmtClass) {
        [self log:@"    건너뜀"];
        return;
    }
    NSString *html = @"<html><body style='font-size:40pt'>한글 가나다 漢字 ABC 123</body></html>";
    id formatter = ((id (*)(id, SEL, id))objc_msgSend)
        ([fmtClass alloc], NSSelectorFromString(@"initWithMarkupText:"), html);

    NSInteger pages = 0;
    NSData *data = [self paginate:formatter pageCount:&pages];
    if (!data) {
        [self log:@"    실패"];
        return;
    }
    CGDataProviderRef provider = CGDataProviderCreateWithCFData((CFDataRef)data);
    CGPDFDocumentRef doc = CGPDFDocumentCreateWithProvider(provider);
    CGDataProviderRelease(provider);
    if (!doc) {
        [self log:@"    실패 — 되읽기 불가"];
        return;
    }
    CGPDFPageRef page = CGPDFDocumentGetPage(doc, 1);
    CGFloat ratio = page ? [self inkRatioOfPage:page] : 0;
    CGPDFDocumentRelease(doc);

    [self log:[NSString stringWithFormat:@"    한글 한 줄: %ld쪽, 잉크 %.4f", (long)pages, ratio]];
    if (ratio < 0.002) {
        [self log:@"    주의 — 한글이 안 그려졌습니다. 글꼴 지정이 필요합니다."];
    } else {
        [self log:@"    한글이 그려집니다."];
    }
}

#pragma mark - 공통

/// 서식기를 쪽으로 나눠 PDF 로 만든다. 전부 런타임으로 간다.
/// paperRect·printableRect 는 읽기 전용이라 KVC 로 넣는다 —
/// 맨 UIPrintPageRenderer 를 설정하는 문서화된 방법이다.
- (NSData *)paginate:(id)formatter pageCount:(NSInteger *)outPages {
    Class rendererClass = NSClassFromString(@"UIPrintPageRenderer");
    if (!rendererClass || !formatter) { return nil; }

    id renderer = [[rendererClass alloc] init];
    if (!renderer) { return nil; }

    SEL addSel = NSSelectorFromString(@"addPrintFormatter:startingAtPageAtIndex:");
    if (![renderer respondsToSelector:addSel]) {
        [self log:@"    addPrintFormatter:startingAtPageAtIndex: 에 응답하지 않습니다"];
        return nil;
    }
    ((void (*)(id, SEL, id, NSInteger))objc_msgSend)(renderer, addSel, formatter, 0);

    CGRect paperRect = CGRectMake(0, 0, kPageWidth, kPageHeight);
    CGRect printableRect = CGRectInset(paperRect, kPageMargin, kPageMargin);
    @try {
        [renderer setValue:[NSValue valueWithCGRect:paperRect] forKey:@"paperRect"];
        [renderer setValue:[NSValue valueWithCGRect:printableRect] forKey:@"printableRect"];
    } @catch (NSException *e) {
        [self log:[NSString stringWithFormat:@"    KVC 실패: %@", e.reason]];
        return nil;
    }

    // numberOfPages 를 읽는 것이 쪽 나누기를 일으킨다.
    SEL countSel = NSSelectorFromString(@"numberOfPages");
    if (![renderer respondsToSelector:countSel]) {
        [self log:@"    numberOfPages 에 응답하지 않습니다"];
        return nil;
    }
    NSInteger pages = ((NSInteger (*)(id, SEL))objc_msgSend)(renderer, countSel);
    if (pages <= 0) { return nil; }
    if (outPages) { *outPages = pages; }

    SEL drawSel = NSSelectorFromString(@"drawPageAtIndex:inRect:");
    if (![renderer respondsToSelector:drawSel]) {
        [self log:@"    drawPageAtIndex:inRect: 에 응답하지 않습니다"];
        return nil;
    }

    // UIGraphicsPDFRenderer 도 tvOS 에서 막혀 있을 수 있으니
    // CoreGraphics 의 PDF 컨텍스트로 직접 찍는다. 이쪽은 확실히 쓸 수 있다.
    NSMutableData *out = [NSMutableData data];
    CGDataConsumerRef consumer = CGDataConsumerCreateWithCFData((CFMutableDataRef)out);
    if (!consumer) { return nil; }
    CGContextRef pdfCtx = CGPDFContextCreate(consumer, &paperRect, NULL);
    CGDataConsumerRelease(consumer);
    if (!pdfCtx) { return nil; }

    for (NSInteger i = 0; i < pages; i++) {
        CGPDFContextBeginPage(pdfCtx, NULL);
        UIGraphicsPushContext(pdfCtx);
        // 인쇄 좌표계는 위에서 아래로 간다. PDF 는 아래에서 위로 간다.
        CGContextTranslateCTM(pdfCtx, 0, kPageHeight);
        CGContextScaleCTM(pdfCtx, 1, -1);
        ((void (*)(id, SEL, NSInteger, CGRect))objc_msgSend)(renderer, drawSel, i, paperRect);
        UIGraphicsPopContext();
        CGPDFContextEndPage(pdfCtx);
    }
    CGPDFContextClose(pdfCtx);
    CGContextRelease(pdfCtx);

    return out.length > 0 ? out : nil;
}

@end
