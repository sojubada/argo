//
//  WebDocumentShell.m
//  Argo
//

#import "WebDocumentShell.h"

@implementation ArgoWebDocumentShell

+ (NSArray<NSString *> *)scriptsForExtension:(NSString *)ext {
    ext = ext.lowercaseString;
    if ([ext isEqualToString:@"docx"]) { return @[ @"jszip.min", @"docx-preview.min" ]; }
    if ([ext isEqualToString:@"xlsx"] || [ext isEqualToString:@"xls"]) { return @[ @"xlsx.min" ]; }
    if ([ext isEqualToString:@"pptx"]) { return @[ @"pptx.min" ]; }
    if ([ext isEqualToString:@"hwp"]) { return @[ @"hwp.min" ]; }
    if ([ext isEqualToString:@"pdf"]) { return @[ @"pdf.min" ]; }
    return @[];
}

// Each renderer gets its own body, and each defines the same small
// navigator so the remote control does not need to know which is which.
+ (NSString *)bodyForExtension:(NSString *)ext {
    ext = ext.lowercaseString;

    if ([ext isEqualToString:@"docx"]) {
        return @"docx.renderAsync(blob,C,null,"
                "{className:'docx',inWrapper:true,breakPages:true,"
                " experimental:true,useBase64URL:true})"
                ".then(function(){"
                " var pages=C.querySelectorAll('section.docx');"
                " nav.units=function(){return pages;};"
                " installScreenPaging('화면');"
                " nav.read(1);"
                " ready(pages.length+'쪽');"
                "}).catch(fail);";
    }

    if ([ext isEqualToString:@"xlsx"] || [ext isEqualToString:@"xls"]) {
        // Every sheet is rendered, then all but one hidden: switching is
        // instant and a big workbook is parsed only once.
        return @"var wb=XLSX.read(arr,{type:'array'});"
                "var names=wb.SheetNames;"
                "names.forEach(function(name){"
                " var d=document.createElement('div');"
                " d.className='sheet';"
                " d.innerHTML=XLSX.utils.sheet_to_html(wb.Sheets[name],{editable:false});"
                " C.appendChild(d);"
                "});"
                "var sheets=C.querySelectorAll('.sheet');"
                "function showOnly(k){"
                " for(var i=0;i<sheets.length;i++){"
                "  sheets[i].style.display=(i===k)?'block':'none';"
                " }"
                " window.scrollTo(0,0);"
                "}"
                "nav.units=function(){return sheets;};"
                "nav.label=function(){return (nav.i+1)+'/'+names.length+'  '+names[nav.i];};"
                "nav.go=function(k){"
                " if(k<0||k>=sheets.length)return;"
                " nav.i=k; showOnly(k); report();"
                "};"
                "showOnly(0);"
                "ready(names.length+' 시트');";
    }

    if ([ext isEqualToString:@"pptx"]) {
        // The master is not rendered, so a deck whose theme writes white
        // text for a dark master comes out white on white. The toolbar
        // can paint the slide, and the page guesses on the way in.
        return @"var p=pptxPreview.init(C,{width:1180,height:664});"
                "Promise.resolve(p.preview(arr.buffer)).then(function(){"
                " var box=C.querySelector('.pptx-preview-wrapper')||C;"
                " var slides=C.querySelectorAll('[class*=slide-wrapper]');"
                " nav.units=function(){return slides;};"
                " nav.label=function(){return (nav.i+1)+' / '+slides.length+' 장';};"
                " nav.go=function(k){"
                "  if(k<0||k>=slides.length)return;"
                "  nav.i=k;"
                "  box.scrollTop=slides[k].offsetTop-slides[0].offsetTop;"
                "  report();"
                " };"
                " nav.scroll=function(dy){box.scrollTop+=dy;report();};"
                " nav.paint=function(css){"
                "  var id='argo-slidebg';"
                "  var st=document.getElementById(id);"
                "  if(!st){st=document.createElement('style');st.id=id;"
                "   document.head.appendChild(st);}"
                "  st.textContent=css?('.slide-background{background:'+css+' !important;}'):'';"
                " };"
                " ready(slides.length+'장');"
                "}).catch(fail);";
    }

    if ([ext isEqualToString:@"hwp"]) {
        return @"(function(){"
                " try{ new hwpjs.Viewer(C,arr,{type:'array'}); }"
                " catch(e1){"
                "  try{ new hwpjs.Viewer(C,window.__argoDoc,{type:'base64'}); }"
                "  catch(e2){ fail(e2); return; }"
                " }"
                " var pages=C.querySelectorAll('[class*=page]');"
                " if(!pages.length){pages=C.querySelectorAll('div > div');}"
                " nav.units=function(){return pages;};"
                " installScreenPaging('화면');"
                " nav.read(1);"
                " ready(pages.length+'쪽');"
                "})();";
    }

    if ([ext isEqualToString:@"pdf"]) {
        // Every page is rasterised once at twice the CSS size, which is
        // what a 55-inch screen at three metres needs, and kept. A long
        // document would be slow drawn all at once, so pages are added
        // a few at a time and the rest follow after the first is up.
        return @"pdfjsLib.GlobalWorkerOptions.workerSrc='pdf.worker.min.js';"
                "pdfjsLib.getDocument({data:arr}).promise.then(function(pdf){"
                " var total=pdf.numPages;"
                " var boxes=[];"
                " for(var n=1;n<=total;n++){"
                "  var d=document.createElement('div');"
                "  d.className='pdfpage';d.dataset.n=n;"
                "  C.appendChild(d);boxes.push(d);"
                " }"
                " nav.units=function(){return boxes;};"
                " nav.label=function(){return (nav.i+1)+' / '+total+' 쪽';};"
                " function draw(n){"
                "  return pdf.getPage(n).then(function(page){"
                "   var vp=page.getViewport({scale:1});"
                "   var scale=Math.min(2.4,(window.innerWidth-80)/vp.width*2);"
                "   var v=page.getViewport({scale:scale});"
                "   var cv=document.createElement('canvas');"
                "   cv.width=v.width;cv.height=v.height;"
                "   cv.style.width=(v.width/2)+'px';"
                "   cv.style.height=(v.height/2)+'px';"
                "   boxes[n-1].appendChild(cv);"
                "   return page.render({canvasContext:cv.getContext('2d'),viewport:v}).promise;"
                "  });"
                " }"
                " function drawFrom(n){"
                "  if(n>total)return Promise.resolve();"
                "  return draw(n).then(function(){"
                "   return new Promise(function(r){setTimeout(function(){"
                "    drawFrom(n+1).then(r);},0);});"
                "  }).catch(function(){});"
                " }"
                " return draw(1).then(function(){"
                "  ready(total+'쪽');"
                "  drawFrom(2);"
                " });"
                "}).catch(fail);";
    }

    return @"fail(new Error('지원하지 않는 형식'));";
}

+ (NSString *)pageForExtension:(NSString *)ext
                    scriptTags:(NSString *)scriptTags
                      fileName:(NSString *)fileName {
    NSString *safeName = fileName ?: @"";
    safeName = [safeName stringByReplacingOccurrencesOfString:@"<" withString:@"&lt;"];

    return [NSString stringWithFormat:
    @"<!doctype html><html><head><meta charset='utf-8'>"
    "<meta name='viewport' content='width=device-width,initial-scale=1'>"
    "<style>"
    // The chrome must not colour the document: a renderer inherits from
    // its container, and white text on a white page reads as a failure.
    "html,body{margin:0;padding:0;background:#4a4a4a;}"
    "#c{color:#000;position:relative;min-height:100%%;}"
    "#c.night{filter:invert(1) hue-rotate(180deg);}"
    "body.night{background:#111;}"
    ".docx-wrapper{background:transparent;padding:24px 0;}"
    ".docx-wrapper>section.docx{box-shadow:0 0 16px rgba(0,0,0,.55);margin-bottom:24px;}"
    ".sheet{padding:16px 24px 48px;}"
    ".pdfpage{display:flex;justify-content:center;padding:14px 0;min-height:40px;}"
    ".pdfpage canvas{box-shadow:0 0 16px rgba(0,0,0,.55);background:#fff;}"
    "table{border-collapse:collapse;background:#fff;color:#000;}"
    "td,th{border:1px solid #b0b0b0;padding:6px 12px;font-size:19px;"
    " white-space:nowrap;font-family:-apple-system,sans-serif;}"
    "tr:nth-child(even) td{background:#f4f6f8;}"
    "#err{color:#ff8a8a;padding:40px;font:22px -apple-system,sans-serif;"
    " white-space:pre-wrap;}"
    "</style></head><body>"
    "<div id='err'></div><div id='c'></div>"
    "%@"
    "<script src='data.js'></script>"
    "<script>"
    "var C=document.getElementById('c');"
    "window.__argoState='loading';window.__argoInfo='';window.__argoErr='';"
    "var nav={i:0,units:function(){return [];},"
    " label:function(){return '';},"
    " go:function(k){var u=nav.units();if(k<0||k>=u.length)return;"
    "  nav.i=k;u[k].scrollIntoView(true);report();},"
    " scroll:function(dy){window.scrollBy(0,dy);report();},"
    " next:function(){nav.go(nav.i+1);},"
    " prev:function(){nav.go(nav.i-1);},"
    " night:function(on){C.className=on?'night':'';"
    "  document.body.className=on?'night':'';},"
    // 쪽 하나의 실제 크기를 재서 화면에 맞춥니다. docx 는 pt 로 못 박힌
    // 폭을 갖고 pptx 는 1180x664 로 그려져, 그대로 두면 TV 에서 작게 뜹니다.
    " fit:function(mode){"
    "  var u=nav.units();var el=(u&&u.length)?u[0]:C;"
    "  C.style.zoom=1;"
    "  var w=el.offsetWidth,h=el.offsetHeight;"
    "  if(!w||!h){return;}"
    "  var z=(mode==='w')?(window.innerWidth/w)"
    "       :(mode==='h')?(window.innerHeight/h):1;"
    "  if(!isFinite(z)||z<=0){z=1;}"
    "  C.style.zoom=z;report();},"
    // 읽기 모드.
    //
    // zoom 만 걸면 쪽이 통째로 넓어져 한 줄이 화면 끝까지 갑니다. 3미터
    // 떨어져서는 그 줄을 따라가기가 어렵습니다. 그래서 쪽 폭을 배율로 나눠
    // 미리 좁혀 둡니다 — 글자는 커지고 단 폭은 1500px 근처로 묶입니다.
    " read:function(s){"
    "  var id='argo-read';var st=document.getElementById(id);"
    "  if(!st){st=document.createElement('style');st.id=id;"
    "   document.head.appendChild(st);}"
    "  s=s||1;"
    // 단 폭은 어느 단계에서나 같습니다. 전에는 '보통'에서만 폭 제한을
    // 걸지 않아, 보통(794px) → 125%(1500px) 에서 폭이 두 배로 튀고
    // 그 뒤로는 거의 변하지 않았습니다 — 단계가 고르지 않던 이유입니다.
    "  var col=Math.round(1460/s);"
    "  st.textContent='#c{width:'+col+'px !important;'"
    "   +'margin:0 auto !important;}'"
    // 렌더러마다 쪽을 담는 요소가 달라, 바깥 그릇을 묶고 안쪽은 꽉 채웁니다.
    "   +'section.docx,[class*=page]{width:100% !important;'"
    "   +'min-width:0 !important;max-width:100% !important;'"
    "   +'box-sizing:border-box !important;padding:36px !important;'"
    "   +'margin:0 auto 20px !important;}';"
    "  C.style.zoom=s;report();},"
    " paint:function(){}};"
    "window.argoNav=nav;"
    "function report(){window.__argoInfo=nav.label();}"
    // 한 화면씩 넘기기.
    //
    // docx-preview 와 hwp.js 가 내놓는 '쪽'은 원본의 쪽과 다릅니다 —
    // 명시적 쪽나눔이 있을 때만 쪼개져, 여러 쪽짜리 문서가 1~2 조각으로
    // 나옵니다. 없는 쪽수를 지어내느니 화면 수를 세는 편이 정직합니다.
    "function screens(){return Math.max(1,Math.ceil("
    " document.body.scrollHeight/Math.max(1,window.innerHeight)));}"
    "function installScreenPaging(unit){"
    " nav.go=function(k){var t=screens();if(k<0||k>=t)return;"
    "  nav.i=k;window.scrollTo(0,k*window.innerHeight);report();};"
    " nav.scroll=function(dy){window.scrollBy(0,dy);"
    "  nav.i=Math.round(window.scrollY/Math.max(1,window.innerHeight));report();};"
    " nav.label=function(){return (nav.i+1)+' / '+screens()+' '+unit;};"
    "}"
    "function ready(info){window.__argoInfo=nav.label()||info;"
    " window.__argoState='done';}"
    "function fail(e){window.__argoState='error';"
    " var m=e?((e.name||'Error')+': '+(e.message||String(e))):String(e);"
    " window.__argoErr=m;"
    " document.getElementById('err').textContent='%@\\n\\n'+m;}"
    // A deck whose text is light was drawn for a dark master that did
    // not render. Sampling the text decides which way to paint.
    "function guessSlideFill(){"
    " var sp=C.querySelectorAll('span');"
    " var light=0,dark=0;"
    " for(var i=0;i<Math.min(sp.length,60);i++){"
    "  var m=/rgb\\((\\d+), ?(\\d+), ?(\\d+)/.exec(getComputedStyle(sp[i]).color);"
    "  if(!m)continue;"
    "  var l=(0.299*+m[1]+0.587*+m[2]+0.114*+m[3])/255;"
    "  if(l>0.6)light++; else dark++;"
    " }"
    " return light>dark?'#1f3864':'';"
    "}"
    "try{"
    " var raw=atob(window.__argoDoc);"
    " var arr=new Uint8Array(raw.length);"
    " for(var i=0;i<raw.length;i++){arr[i]=raw.charCodeAt(i);}"
    " var blob=new Blob([arr]);"
    " window.__argoDocBytes=arr.length;"
    " %@"
    "}catch(e){fail(e);}"
    "</script></body></html>",
    scriptTags, safeName, [self bodyForExtension:ext]];
}

@end
