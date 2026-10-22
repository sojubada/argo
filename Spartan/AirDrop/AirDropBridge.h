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
