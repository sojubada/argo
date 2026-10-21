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
                " nav.label=function(){return (nav.i+1)+' / '+pages.length+' 쪽';};"
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
                " nav.paint(guessSlideFill());"
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
                " nav.label=function(){return pages.length?((nav.i+1)+' / '+pages.length+' 쪽'):'';};"
                " ready(pages.length+'쪽');"
                "})();";
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
    " paint:function(){}};"
    "window.argoNav=nav;"
    "function report(){window.__argoInfo=nav.label();}"
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
