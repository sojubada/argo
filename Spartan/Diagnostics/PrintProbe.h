//
//  PrintProbe.h
//  Argo
//
//  tvOS 는 인쇄 서식기를 헤더에 declare 하지만 API_UNAVAILABLE(tvos) 로 막아
//  둡니다. 컴파일러는 거부합니다. 그러나 막혔다는 것과 없다는 것은 다릅니다 —
//  WebKit 이 정확히 그랬습니다: SDK 에 없는데 기기에는 있었습니다.
//
//  그래서 여기서는 인쇄 심볼을 컴파일 시점에 하나도 참조하지 않습니다.
//  전부 NSClassFromString 과 objc_msgSend 로 갑니다. AirDrop 때
//  Sharing.framework, 문서 뷰 때 WebKit 에 썼던 그 방법입니다.
//
//  묻는 것은 '클래스가 있나'가 아니라 '잉크가 나오나' 입니다.
//  둘은 다른 질문이고 두 번째만 중요합니다.
//

#import <UIKit/UIKit.h>

@interface ArgoPrintProbeViewController : UIViewController
@property (nonatomic, copy) void (^onClose)(void);
@end
