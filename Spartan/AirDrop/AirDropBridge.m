//
//  AirDropBridge.m
//  Argo
//

#import "AirDropBridge.h"

NSNotificationName const ArgoAirDropStateChanged = @"ArgoAirDropStateChanged";

#pragma mark - 비공개 클래스 선언 (Sharing.framework)

// 아래 선언은 실기기에서 검증된 것을 그대로 옮긴 것입니다. 변형하지 마십시오.

typedef NS_ENUM(long long, SDAirDropDiscoverableMode) {
    SDAirDropDiscoverableModeOff          = 0,
    SDAirDropDiscoverableModeContactsOnly = 1,
    SDAirDropDiscoverableModeEveryone     = 2,
};

@interface SFAirDropDiscoveryController : UIViewController
- (void)setDiscoverableMode:(long long)mode;
- (long long)discoverableMode;
@end

@interface SFAirDropReceiverViewController : UIViewController
- (void)startAdvertising;
- (void)stopAdvertising;
@end

@interface SFAirDropSharingViewControllerTV : UIViewController
- (instancetype)initWithSharingItems:(NSArray *)items;
- (void)setCompletionHandler:(void (^)(NSError *error))handler;
@end

#pragma mark -

@implementation AirDropBridge

/// 모드가 반영되기까지 기다리는 시간. 실측으로 1초쯤 걸립니다.
static const NSTimeInterval kSettleDelay = 1.6;
/// 받은 것이 늘어나면 이만큼 더 열어 둡니다.
static const NSTimeInterval kTransferExtension = 60.0;
/// 받은 파일을 얼마나 자주 살펴볼지.
static const NSTimeInterval kPollInterval = 2.0;

/// 컨트롤러는 **딱 한 번만** 만들어 들고 있습니다.
/// 호출할 때마다 새로 만들면 모드가 유지되지 않습니다.
static SFAirDropDiscoveryController *sDiscovery = nil;
/// 받기 광고를 띄우는 컨트롤러. 화면 구석에 1픽셀로 붙여 둡니다.
static SFAirDropReceiverViewController *sReceiver = nil;

static NSDate *sDeadline = nil;
static NSTimer *sTimer = nil;
static NSString *sLastInboxFingerprint = nil;

#pragma mark - 사용 가능한가

+ (void)loadFrameworkIfNeeded {
    static dispatch_once_t once;
    dispatch_once(&once, ^{
        // 지연 로드가 필요합니다. 링크하지 않습니다.
        [[NSBundle bundleWithPath:@"/System/Library/PrivateFrameworks/Sharing.framework"] load];
        [[NSBundle bundleWithPath:@"/System/Library/PrivateFrameworks/SharingUI.framework"] load];
    });
}

+ (BOOL)isAvailable {
    [self loadFrameworkIfNeeded];
    // 셋 다 있어야 켜기·받기·보내기가 전부 됩니다. 하나라도 없으면
    // 숨기는 쪽이 맞습니다 — 반쪽짜리 UI 는 없느니만 못합니다.
    return NSClassFromString(@"SFAirDropDiscoveryController") != nil
        && NSClassFromString(@"SFAirDropReceiverViewController") != nil
        && NSClassFromString(@"SFAirDropSharingViewControllerTV") != nil;
}

+ (NSString *)inboxPath {
    return @"/var/mobile/Downloads/com.apple.AirDrop";
}

#pragma mark - 상태

+ (BOOL)isReceiving {
    return sDeadline != nil && [sDeadline timeIntervalSinceNow] > 0;
}

+ (NSTimeInterval)remainingSeconds {
    if (!sDeadline) { return 0; }
    NSTimeInterval left = [sDeadline timeIntervalSinceNow];
    return left > 0 ? left : 0;
}

+ (long long)currentMode {
    if (!sDiscovery) { return 0; }
    @try {
        return [sDiscovery discoverableMode];
    } @catch (NSException *e) {
        NSLog(@"[AirDrop] 모드를 읽지 못했습니다: %@", e.reason);
        return 0;
    }
}

+ (void)notifyChanged {
    dispatch_async(dispatch_get_main_queue(), ^{
        [[NSNotificationCenter defaultCenter] postNotificationName:ArgoAirDropStateChanged
                                                            object:nil];
    });
}

#pragma mark - 켜기

+ (void)enableReceivingForSeconds:(NSTimeInterval)seconds
                       completion:(void (^)(BOOL, long long))completion {
    if (![self isAvailable]) {
        if (completion) { completion(NO, 0); }
        return;
    }

    Class cls = NSClassFromString(@"SFAirDropDiscoveryController");
    if (!sDiscovery) {
        @try {
            sDiscovery = [[cls alloc] init];
        } @catch (NSException *e) {
            NSLog(@"[AirDrop] 컨트롤러를 만들지 못했습니다: %@", e.reason);
            if (completion) { completion(NO, 0); }
            return;
        }
    }
    // sDiscovery 의 view 에는 절대 손대지 않습니다 — 접근하는 순간 앱이 죽습니다.
    // addSubview, present, frame 설정 전부 포함입니다.

    @try {
        // 일부 구현은 Off 에서 곧장 Everyone 으로 가는 전이를 받지 않습니다.
        // 연락처만을 한 번 거쳐 갑니다.
        [sDiscovery setDiscoverableMode:SDAirDropDiscoverableModeContactsOnly];
        [sDiscovery setDiscoverableMode:SDAirDropDiscoverableModeEveryone];
    } @catch (NSException *e) {
        NSLog(@"[AirDrop] 모드를 바꾸지 못했습니다: %@", e.reason);
        if (completion) { completion(NO, 0); }
        return;
    }

    [self attachReceiver];

    sDeadline = [NSDate dateWithTimeIntervalSinceNow:seconds];
    sLastInboxFingerprint = nil;   // 첫 값은 뒷줄에서 채운다
    [self startTimer];
    [self notifyChanged];

    // 설정은 비동기로 반영됩니다. 1.6초 뒤에 확인합니다 —
    // 즉시 읽으면 반드시 0 이 나와 실패로 오판하게 됩니다.
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(kSettleDelay * NSEC_PER_SEC)),
                   dispatch_get_main_queue(), ^{
        long long mode = [self currentMode];
        [self notifyChanged];
        if (completion) { completion(mode == SDAirDropDiscoverableModeEveryone, mode); }
    });
}

/// 받기 광고. 컨트롤러의 뷰가 화면에 올라가 있어야 광고가 시작됩니다.
/// 전체 화면으로 띄우면 파일 탐색기를 못 쓰므로 구석에 1픽셀로 붙입니다.
+ (void)attachReceiver {
    if (sReceiver) { return; }
    Class cls = NSClassFromString(@"SFAirDropReceiverViewController");
    if (!cls) { return; }

    UIViewController *host = [self topViewController];
    if (!host) { return; }

    @try {
        sReceiver = [[cls alloc] init];
        [host addChildViewController:sReceiver];
        sReceiver.view.frame = CGRectMake(0, 0, 1, 1);
        sReceiver.view.clipsToBounds = YES;
        sReceiver.view.userInteractionEnabled = NO;
        sReceiver.view.alpha = 0.02;
        [host.view addSubview:sReceiver.view];
        [sReceiver didMoveToParentViewController:host];

        if ([sReceiver respondsToSelector:@selector(startAdvertising)]) {
            [sReceiver startAdvertising];
        }
    } @catch (NSException *e) {
        NSLog(@"[AirDrop] 받기 광고를 시작하지 못했습니다: %@", e.reason);
        sReceiver = nil;
    }
}

+ (void)detachReceiver {
    if (!sReceiver) { return; }
    @try {
        if ([sReceiver respondsToSelector:@selector(stopAdvertising)]) {
            [sReceiver stopAdvertising];
        }
        [sReceiver willMoveToParentViewController:nil];
        [sReceiver.view removeFromSuperview];
        [sReceiver removeFromParentViewController];
    } @catch (NSException *e) {
        NSLog(@"[AirDrop] 받기 광고를 끄지 못했습니다: %@", e.reason);
    }
    sReceiver = nil;
}

#pragma mark - 끄기

+ (void)disableReceiving {
    [sTimer invalidate];
    sTimer = nil;
    sDeadline = nil;

    [self detachReceiver];

    if (sDiscovery) {
        @try {
            [sDiscovery setDiscoverableMode:SDAirDropDiscoverableModeOff];
        } @catch (NSException *e) {
            NSLog(@"[AirDrop] 끄지 못했습니다: %@", e.reason);
        }
    }
    [self notifyChanged];
}

#pragma mark - 시계와 전송 감지

+ (void)startTimer {
    [sTimer invalidate];
    sTimer = [NSTimer scheduledTimerWithTimeInterval:kPollInterval
                                             repeats:YES
                                               block:^(NSTimer *t) {
        [self tick];
    }];
}

+ (void)tick {
    // 디스크를 훑는 일은 뒷줄에서 한다.
    //
    // 전에는 여기서 바로 inboxFingerprint 를 불렀다. 타이머가 메인 런루프에
    // 붙어 있고 그 함수는 enumeratorAtPath 로 동기 디스크 I/O 를 하므로,
    // 2초마다 메인 스레드가 받은 함을 통째로 훑었다 — 받은 것이 쌓일수록
    // 화면이 끊긴다. 개수를 4,000개로 끊어 둔 것은 양을 줄였을 뿐
    // 위치가 틀린 것은 고치지 못한다.
    dispatch_async(dispatch_get_global_queue(QOS_CLASS_UTILITY, 0), ^{
        NSString *now = [self inboxFingerprint];
        dispatch_async(dispatch_get_main_queue(), ^{
            [self applyFingerprint:now];
        });
    });
}

/// 뒷줄에서 읽어 온 결과를 메인에서 적용한다. 상태는 전부 여기서만 바꾼다.
+ (void)applyFingerprint:(NSString *)now {
    // 받은 것이 늘었으면 전송 중이다. 시간을 다시 채운다 —
    // 한창 받는 중에 꺼지면 파일이 반쪽으로 남는다.
    //
    // 다만 이 신호는 약하다. 받는 쪽은 보통 완성될 때까지 임시 자리에 쓰고
    // 끝난 뒤에야 받은 함으로 옮기므로, 큰 파일을 받는 내내 여기는
    // 아무 변화가 없을 수 있다. 수신 화면이 떠 있는 동안 열어 두는 쪽으로
    // 바꿔야 한다 — 그때까지의 임시 장치다.
    if (now && sLastInboxFingerprint && ![now isEqualToString:sLastInboxFingerprint]) {
        sDeadline = [NSDate dateWithTimeIntervalSinceNow:kTransferExtension];
        NSLog(@"[AirDrop] 받는 중 — 시간을 연장합니다");
    }
    sLastInboxFingerprint = now;

    if (!sDeadline || [sDeadline timeIntervalSinceNow] <= 0) {
        [self disableReceiving];
        return;
    }
    [self notifyChanged];
}

/// 받은 함의 상태를 한 줄로 요약한다. 바뀌면 뭔가 들어오고 있다는 뜻이다.
/// 파일시스템 알림 대신 들여다보는 이유는, 여기서는 알림이 오지 않는
/// 경우가 있어 이쪽이 확실하기 때문이다.
+ (NSString *)inboxFingerprint {
    NSFileManager *fm = [NSFileManager defaultManager];
    NSString *root = [self inboxPath];
    if (![fm fileExistsAtPath:root]) { return NSLocalizedString(@"없음", nil); }

    unsigned long long total = 0;
    NSUInteger count = 0;
    NSDirectoryEnumerator *e = [fm enumeratorAtPath:root];
    for (NSString *rel in e) {
        NSDictionary *attrs = [e fileAttributes];
        if ([attrs[NSFileType] isEqual:NSFileTypeRegular]) {
            total += [attrs[NSFileSize] unsignedLongLongValue];
            count++;
        }
        if (count > 4000) { break; }   // 받은 함이 커져도 매 2초가 무거워지지 않게
    }
    return [NSString stringWithFormat:@"%lu:%llu", (unsigned long)count, total];
}

#pragma mark - 보내기

+ (void)presentSenderForFileAtPath:(NSString *)path
                        completion:(void (^)(NSError * _Nullable))completion {
    if (![self isAvailable] || path.length == 0) {
        if (completion) { completion([NSError errorWithDomain:@"Argo" code:1 userInfo:@{
            NSLocalizedDescriptionKey: NSLocalizedString(@"이 기기에서는 AirDrop 을 쓸 수 없습니다.", nil)
        }]); }
        return;
    }

    BOOL isDir = NO;
    if (![[NSFileManager defaultManager] fileExistsAtPath:path isDirectory:&isDir] || isDir) {
        if (completion) { completion([NSError errorWithDomain:@"Argo" code:2 userInfo:@{
            NSLocalizedDescriptionKey: NSLocalizedString(@"폴더는 보낼 수 없습니다. 먼저 압축하십시오.", nil)
        }]); }
        return;
    }

    Class cls = NSClassFromString(@"SFAirDropSharingViewControllerTV");
    UIViewController *host = [self topViewController];
    if (!cls || !host) {
        if (completion) { completion([NSError errorWithDomain:@"Argo" code:3 userInfo:@{
            NSLocalizedDescriptionKey: NSLocalizedString(@"보내기 화면을 열 수 없습니다.", nil)
        }]); }
        return;
    }

    @try {
        NSURL *url = [NSURL fileURLWithPath:path];
        SFAirDropSharingViewControllerTV *sharing =
            [[cls alloc] initWithSharingItems:@[ url ]];

        __weak UIViewController *weakSharing = sharing;
        if ([sharing respondsToSelector:@selector(setCompletionHandler:)]) {
            [sharing setCompletionHandler:^(NSError *error) {
                dispatch_async(dispatch_get_main_queue(), ^{
                    [weakSharing dismissViewControllerAnimated:YES completion:nil];
                    if (completion) { completion(error); }
                });
            }];
        }
        [host presentViewController:sharing animated:YES completion:nil];
    } @catch (NSException *e) {
        if (completion) { completion([NSError errorWithDomain:@"Argo" code:4 userInfo:@{
            NSLocalizedDescriptionKey: e.reason ?: NSLocalizedString(@"보내기에 실패했습니다.", nil)
        }]); }
    }
}

#pragma mark - 지금 화면

+ (UIViewController *)topViewController {
    UIWindow *window = nil;
    for (UIWindow *w in [UIApplication sharedApplication].windows) {
        if (w.isKeyWindow) { window = w; break; }
    }
    if (!window) { window = [UIApplication sharedApplication].windows.firstObject; }

    UIViewController *vc = window.rootViewController;
    while (vc.presentedViewController) { vc = vc.presentedViewController; }
    return vc;
}

@end
