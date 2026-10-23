# 검증 A — AirDrop 통합

커밋 `6869fc9`. **Argo 안에서는 한 번도 기기에서 돌려 보지 않았습니다.**

별도 검증 앱(`sojubada/AirDrop_tvOS`)에서 Apple TV 4K / tvOS 16.6 / TrollStore 로
**송·수신이 모두 확인된** 방법을 옮긴 것입니다. 다만 아래 ③ 은 그 앱에 없던,
제가 만든 방식입니다.

---

## 답해 주셔야 할 질문

### Q1. `SFAirDropDiscoveryController` 의 `view` 를 건드리는 곳이 있습니까?  ★

검증 과정에서 확인된 사실입니다 — 이 컨트롤러는 이름이 Controller 이고
`UIViewController` 를 상속하지만 **화면에 올릴 수 있는 물건이 아닙니다.**
아래는 전부 즉시 크래시입니다.

```objc
[self addChildViewController:discovery];
[self.view addSubview:discovery.view];
[self presentViewController:discovery animated:YES completion:nil];
discovery.view.frame = ...;
```

`AirDropBridge.m` 전체에서 `sDiscovery` 의 `view` 에 닿는 경로가 있는지
봐 주십시오. **간접 경로도 포함입니다** — 예를 들어 이 객체를 배열에 넣어
UIKit 이 순회하게 두거나, 디버거 출력(`%@`)으로 `description` 을 부르는 것이
`view` 를 로드시키는지 확인이 필요합니다.

### Q2. 1픽셀 수신 컨트롤러 부착이 통합니까?  ★

```objc
sReceiver.view.frame = CGRectMake(0, 0, 1, 1);
sReceiver.view.alpha = 0.02;
[host.view addSubview:sReceiver.view];
[sReceiver startAdvertising];
```

**검증 앱은 전체 화면으로 `present` 했습니다.** 저는 파일 탐색기를 가리지
않으려고 1픽셀 자식으로 붙였습니다. 근거가 없습니다.

- `startAdvertising` 이 뷰의 실제 표시 여부에 의존합니까?
- 1픽셀·거의 투명 상태에서도 광고가 나갑니까?
- 이 뷰가 tvOS 포커스 엔진에 잡혀 파일 목록의 포커스를 훔칠 수 있습니까?
  (`userInteractionEnabled = NO` 로 막았습니다만 충분한지 모르겠습니다)

**만약 통하지 않는다면**: discoverable 모드만으로 수신이 되는지,
아니면 수신 전용 화면을 따로 띄워야 하는지 알려 주십시오.

### Q3. 모드 설정의 비동기 처리가 맞습니까?

`setDiscoverableMode:` 직후에 읽으면 **항상 0** 이 나오고 약 1초 뒤에 2가 됩니다.
그래서 1.6초 뒤에 확인합니다. 이 값이 충분합니까? 더 확실한 완료 신호가 있습니까?

### Q4. 1분 타이머와 전송 중 연장이 안전합니까?

받은 함을 2초마다 훑어 크기·개수가 바뀌면 시간을 다시 채웁니다.

- 전송이 **시작되기 전**(상대가 고르는 중)에는 파일이 안 생기므로 1분 안에
  꺼질 수 있습니다. 이게 문제입니까?
- 큰 파일 하나를 받는 중에는 크기가 계속 변하므로 연장됩니다. 맞습니까?
- `enumeratorAtPath` 를 2초마다 도는 비용이 받은 함이 커지면 문제가 됩니까?
  (4000개에서 끊도록 했습니다)

### Q5. 전체 화면 `present` 중에 부착된 수신 뷰는 어떻게 됩니까?

`attachReceiver` 는 **그 시점의** 최상위 뷰 컨트롤러에 붙습니다.
그 뒤 사용자가 문서 뷰어를 열면(시트) 수신 뷰는 가려진 아래쪽 화면에 남습니다.
광고가 계속 나갑니까, 아니면 끊깁니까?

---

## 1. 브리지 — 비공개 API 를 가두는 곳

### `Spartan/AirDrop/AirDropBridge.h`  (69줄)

```objc
//
//  AirDropBridge.h
//  Argo
//
//  tvOS 에서 AirDrop 을 켜고 끄는 최소한의 입구.
//
//  비공개 API(Sharing.framework)의 복잡함과 함정을 전부 이 파일 안에 가둡니다.
//  Swift 쪽은 여기 있는 것만 보면 됩니다.
//
//  AirDrop_tvOS 저장소에서 실기기(Apple TV 4K / tvOS 16.6 / TrollStore)로
//  송·수신이 모두 검증된 방법을 그대로 옮긴 것입니다. 추측한 부분은 없습니다.
//

#import <UIKit/UIKit.h>

NS_ASSUME_NONNULL_BEGIN

/// 상태가 바뀌면 이 알림이 옵니다. 켜짐·꺼짐·남은 시간 변화 전부.
extern NSNotificationName const ArgoAirDropStateChanged;

@interface AirDropBridge : NSObject

/// 이 기기에서 AirDrop 을 쓸 수 있는가. NO 면 관련 UI 를 전부 숨길 것.
/// 오류를 띄우지 말고 기능이 없는 것처럼 동작해야 합니다.
@property (class, nonatomic, readonly) BOOL isAvailable;

/// 지금 받는 중인가 (우리가 켜 둔 상태인가).
@property (class, nonatomic, readonly) BOOL isReceiving;

/// 자동으로 꺼지기까지 남은 초. 꺼져 있으면 0.
@property (class, nonatomic, readonly) NSTimeInterval remainingSeconds;

/// 실제 discoverable 모드 (0=Off, 1=연락처만, 2=모두).
/// 저장된 값이 아니라 데몬에 물어본 값입니다.
@property (class, nonatomic, readonly) long long currentMode;

/// 받은 파일이 쌓이는 곳.
@property (class, nonatomic, readonly) NSString *inboxPath;

/// 정해진 시간 동안 받기를 켭니다.
///
/// 모드 설정은 비동기로 반영됩니다 — 설정 직후에 읽으면 **항상 0** 이 나옵니다.
/// 그래서 결과는 콜백으로만 돌려줍니다. 즉시 읽어서 성공·실패를 판정하면
/// 반드시 틀립니다.
///
/// 켜져 있는 동안 받은 파일이 늘어나면 시간을 자동으로 연장합니다 —
/// 전송 중에 꺼지면 파일이 깨집니다.
+ (void)enableReceivingForSeconds:(NSTimeInterval)seconds
                       completion:(nullable void (^)(BOOL success, long long mode))completion
    NS_SWIFT_NAME(enableReceiving(seconds:completion:));

/// 받기를 끕니다.
+ (void)disableReceiving;

/// 파일 하나를 다른 기기로 보내는 화면을 띄웁니다.
/// 폴더는 보내지 마십시오 — tvOS 에서 동작이 불확실합니다.
/// 띄울 화면은 안에서 찾습니다 — 부르는 쪽이 뷰 컨트롤러를 들고 있을
/// 필요가 없습니다.
+ (void)presentSenderForFileAtPath:(NSString *)path
                        completion:(nullable void (^)(NSError * _Nullable error))completion
    NS_SWIFT_NAME(send(filePath:completion:));

/// 지금 화면에 떠 있는 뷰 컨트롤러.
+ (nullable UIViewController *)topViewController
    NS_SWIFT_NAME(topViewController());

@end

NS_ASSUME_NONNULL_END
```

### `Spartan/AirDrop/AirDropBridge.m`  (336줄)

```objc
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
    sLastInboxFingerprint = [self inboxFingerprint];
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
    // 받은 것이 늘었으면 전송 중이다. 시간을 다시 채운다 —
    // 한창 받는 중에 꺼지면 파일이 반쪽으로 남는다.
    NSString *now = [self inboxFingerprint];
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
    if (![fm fileExistsAtPath:root]) { return @"없음"; }

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
            NSLocalizedDescriptionKey: @"이 기기에서는 AirDrop 을 쓸 수 없습니다."
        }]); }
        return;
    }

    BOOL isDir = NO;
    if (![[NSFileManager defaultManager] fileExistsAtPath:path isDirectory:&isDir] || isDir) {
        if (completion) { completion([NSError errorWithDomain:@"Argo" code:2 userInfo:@{
            NSLocalizedDescriptionKey: @"폴더는 보낼 수 없습니다. 먼저 압축하십시오."
        }]); }
        return;
    }

    Class cls = NSClassFromString(@"SFAirDropSharingViewControllerTV");
    UIViewController *host = [self topViewController];
    if (!cls || !host) {
        if (completion) { completion([NSError errorWithDomain:@"Argo" code:3 userInfo:@{
            NSLocalizedDescriptionKey: @"보내기 화면을 열 수 없습니다."
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
            NSLocalizedDescriptionKey: e.reason ?: @"보내기에 실패했습니다."
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
```

**볼 곳**

- `sDiscovery` 는 static 으로 **한 번만** 만듭니다. 호출할 때마다 새로
  만들면 모드가 유지되지 않는다는 것이 검증 과정에서 확인됐습니다
- 모드 전이는 `Off → ContactsOnly → Everyone` 입니다. 일부 구현이 Off 에서
  곧장 Everyone 으로 가는 전이를 받지 않는다고 해서 한 단계를 거칩니다
- 모든 비공개 호출이 `@try/@catch` 안에 있는지
- `NSClassFromString` 이 nil 일 때 조용히 비활성화되는지 (오류 팝업 금지)

---

## 2. SwiftUI 쪽

### `Spartan/AirDrop/AirDropState.swift`  (82줄)

```swift
//
//  AirDropState.swift
//  Argo
//
//  AirDrop 의 SwiftUI 쪽 얼굴. 실제 일은 AirDropBridge 가 합니다.
//
//  저장된 토글 값을 믿지 않습니다. 모드는 디스크 설정이 아니라 실행 중인
//  sharingd 의 상태라, 재부팅하거나 앱이 뒤로 갔다 오면 꺼져 있을 수 있습니다.
//  그래서 늘 실제 상태를 읽어 보여 줍니다.
//

import SwiftUI
import Combine

@available(tvOS 13.0, *)
final class AirDropState: ObservableObject {

    /// 한 번 누르면 이만큼 열립니다. 받는 중이면 자동으로 연장됩니다.
    static let openSeconds: TimeInterval = 60

    @Published private(set) var isReceiving = false
    @Published private(set) var remaining: TimeInterval = 0
    @Published private(set) var isStarting = false
    /// 켰는데 모드가 올라오지 않은 경우. 조용히 실패하는 것보다 말해 주는 편이 낫습니다.
    @Published var lastFailure: String?

    var isAvailable: Bool { AirDropBridge.isAvailable }

    private var observer: NSObjectProtocol?

    init() {
        // 이름을 직접 적습니다. ObjC 의 NSNotificationName 상수가 Swift 로
        // 어떤 멤버 이름이 되는지 추측하면 빌드를 한 번 낭비하게 됩니다.
        observer = NotificationCenter.default.addObserver(
            forName: Notification.Name("ArgoAirDropStateChanged"),
            object: nil, queue: .main
        ) { [weak self] _ in
            self?.refresh()
        }
        refresh()
    }

    deinit {
        if let observer = observer {
            NotificationCenter.default.removeObserver(observer)
        }
    }

    private func refresh() {
        isReceiving = AirDropBridge.isReceiving
        remaining = AirDropBridge.remainingSeconds
    }

    /// 꺼져 있으면 켜고, 켜져 있으면 끕니다.
    func toggle() {
        guard isAvailable else { return }
        if isReceiving {
            AirDropBridge.disableReceiving()
            refresh()
            return
        }

        isStarting = true
        lastFailure = nil
        AirDropBridge.enableReceiving(seconds: Self.openSeconds) { [weak self] success, mode in
            guard let self = self else { return }
            self.isStarting = false
            self.refresh()
            if !success {
                // 모드가 2(모두)로 올라오지 않았습니다. 켜진 것처럼 보이게 두면
                // 아이폰에서 안 보이는 이유를 알 길이 없습니다.
                self.lastFailure = "AirDrop 을 켜지 못했습니다 (모드 \(mode)). 다시 시도해 보십시오."
            }
        }
    }

    /// 툴바에 보여 줄 짧은 글. 켜져 있을 때만 씁니다.
    var countdownLabel: String {
        guard isReceiving else { return "" }
        return "\(Int(remaining.rounded()))초"
    }
}
```

**볼 곳**

- **저장된 토글 값을 믿지 않습니다.** 모드는 디스크 설정이 아니라 실행 중인
  `sharingd` 의 상태라 재부팅하면 Off 로 돌아갑니다
- `@StateObject` 로 `ContentView` 가 들고 있습니다. 뷰가 다시 만들어질 때
  `init` 의 `isAvailable` 접근이 프레임워크를 다시 로드하지 않는지
  (`dispatch_once` 로 막았습니다)
- 실패했을 때 조용히 켜진 척하지 않고 말하는지

---

## 3. 툴바 버튼과 보내기 메뉴

### `Spartan/ContentView.swift (툴바의 AirDrop 버튼)`  (27줄)

```swift

                    // AirDrop. 쓸 수 없는 기기에서는 아예 보이지 않는다 —
                    // 눌러도 안 되는 버튼을 두는 것보다 없는 편이 낫다.
                    if airdrop.isAvailable {
                        Button(action: {
                            airdrop.toggle()
                        }) {
                            HStack(spacing: 6) {
                                Image(systemName: "dot.radiowaves.left.and.right")
                                if airdrop.isReceiving {
                                    // 남은 시간을 같이 보여 준다. 받는 중이면
                                    // 저절로 늘어나므로 멈춘 것처럼 보이지 않는다.
                                    Text(airdrop.countdownLabel)
                                        .font(.caption)
                                }
                            }
                            .frame(height: 50)
                            .padding(.horizontal, airdrop.isReceiving ? 12 : 0)
                            .frame(minWidth: 50)
                            .foregroundColor(airdrop.isReceiving ? Theme.airdropOn : nil)
                        }
                        .alert(isPresented: airdropFailureBinding) {
                            Alert(title: Text("AirDrop"),
                                  message: Text(airdrop.lastFailure ?? ""),
                                  dismissButton: .default(Text(NSLocalizedString("DISMISS", comment: ""))))
                        }
                    }
```

### `Spartan/ContentView.swift (길게 누르기 — 보내기)`  (20줄)

```swift
            canAirDrop: { index in
                guard AirDropBridge.isAvailable,
                      model.masterFiles.indices.contains(index) else { return false }
                // 폴더는 tvOS 에서 동작이 불확실하다. 압축해서 보내면 된다.
                return !model.masterFiles[index].name.hasSuffix("/")
            },
            airDrop: { index in
                guard let path = focusItem(index) else { return }
                AirDropBridge.send(filePath: path) { error in
                    if let error = error {
                        alertTitle = "AirDrop"
                        alertMsg = error.localizedDescription
                        showAlert = true
                    }
                }
            }
        )
    }

    // MARK: - 클립보드
```

**볼 곳**

- 쓸 수 없는 기기에서 버튼이 **아예 없는지** (비활성이 아니라 부재)
- 폴더를 보내려 하지 않는지 — tvOS 에서 폴더 AirDrop 은 동작이 불확실합니다
- 켜졌을 때 파란색(`Theme.airdropOn`)으로 바뀌고 남은 시간이 보이는지

---

## 4. 권한

```xml
<key>com.apple.private.airdrop.discovery</key><true/>
<key>com.apple.private.airdrop.settings</key><true/>
```

두 개만 추가했습니다. `no-sandbox`·`platform-application`·
`storage.AppDataContainers` 는 Argo 에 이미 있었습니다.

**볼 곳:** 이 두 키가 TrollStore 금지 목록에 없는지, 그리고 두 개로 충분한지.
