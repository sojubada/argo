#!/usr/bin/env python3
"""P1b 시험 파일 생성: 수식을 xlsx 에 쓰고 LibreOffice 로 재계산해 정답 JSON 을 만든다.
쓰는 법: python3 tools/make_p1b_corpus.py <출력 폴더>
결과: p1b_corpus.xlsx (수식만, 값 없음) / p1b_golden.json (LibreOffice 계산값)
"""
import json, os, subprocess, sys, tempfile, shutil, datetime
import openpyxl

OUT = sys.argv[1]
os.makedirs(OUT, exist_ok=True)

# 데이터 칸 (A열: 숫자·글자·빈 칸 섞음, B열: 날짜 일련번호, C열: 글자)
DATA = {
    "A1": 10, "A2": 20, "A3": "x", "A4": 30, "A5": None, "A6": True, "A7": -5, "A8": 2.5,
    "B1": 46022, "B2": 46053, "B3": 46081, "B4": 45717, "B5": 45351,
    "C1": "Apple", "C2": "banana", "C3": "Cherry", "C4": "apple",
    "D1": 1, "D2": 2, "D3": 3, "D4": 4, "D5": 5,
    "E1": "a", "E2": "b", "E3": "c", "E4": "d", "E5": "e",
}

F = [
 # 수학
 "PRODUCT(2,3,4)", "PRODUCT(A1:A4)", "PRODUCT(\"2\",3)",
 "ROUNDUP(3.14159,1)", "ROUNDUP(-3.14159,1)", "ROUNDUP(31415.9,-2)", "ROUNDUP(0.1+0.2,1)", "ROUNDUP(2.0000000001,0)",
 "ROUNDDOWN(3.14159,1)", "ROUNDDOWN(-3.14159,1)", "ROUNDDOWN(31415.9,-2)", "ROUNDDOWN(1.005*1000,0)",
 "TRUNC(3.9)", "TRUNC(-3.9)", "TRUNC(8.987,2)", "INT(-3.2)", "INT(3.999)",
 "MOD(-3,2)", "MOD(3,-2)", "MOD(5.5,2)", "MOD(10,0)", "MOD(-7.5,2)",
 "SIGN(42)", "SIGN(-0.1)", "SIGN(0)", "POWER(2,3)", "POWER(2,0.5)", "POWER(-8,1/3)", "POWER(0,-1)",
 "SQRT(25)", "SQRT(-1)", "CEILING(4.2,1)", "CEILING(-4.2,-1)", "CEILING(4.2,0.5)", "CEILING(-4.2,1)", "CEILING(4.2,-1)",
 "_xlfn.CEILING.MATH(-4.2)", "_xlfn.CEILING.MATH(-4.2,1,1)", "_xlfn.CEILING.MATH(4.2,2)",
 "FLOOR(4.8,1)", "FLOOR(-4.8,-1)", "FLOOR(4.8,0.5)", "FLOOR(-4.8,1)",
 "_xlfn.FLOOR.MATH(-4.8)", "_xlfn.FLOOR.MATH(-4.8,1,1)", "_xlfn.FLOOR.MATH(4.8,2)",
 "MROUND(11,5)", "MROUND(12.5,5)", "MROUND(-11,-5)", "MROUND(11,-5)", "MROUND(1.3,0.2)",
 "EXP(1)", "LN(EXP(1))", "LN(0)", "LOG(1000,10)", "LOG(8)", "LOG(8,2)", "LOG10(100)", "LOG10(0.001)", "PI()",
 # 통계
 "AVERAGEA(A1:A8)", "COUNTBLANK(A1:A8)", "COUNTBLANK(C1:C5)",
 "_xlfn.MINIFS(D1:D5,E1:E5,\">b\")", "_xlfn.MAXIFS(D1:D5,E1:E5,\"<d\")", "_xlfn.MAXIFS(D1:D5,E1:E5,\"z\")",
 "_xlfn.RANK.EQ(3,D1:D5)", "_xlfn.RANK.EQ(3,D1:D5,1)",
 # 논리
 "_xlfn.IFS(1=2,10,2=2,20)", "_xlfn.IFS(1=2,10)", "_xlfn.SWITCH(2,1,10,2,20,30)", "_xlfn.SWITCH(9,1,10,2,20,30)", "_xlfn.SWITCH(9,1,10)",
 "NOT(TRUE)", "NOT(0)", "_xlfn.XOR(TRUE,FALSE,FALSE)", "_xlfn.XOR(TRUE,TRUE)", "_xlfn.XOR(D1:D3)",
 "_xlfn.IFNA(NA(),999)", "_xlfn.IFNA(1/0,999)",
 # 정보
 "ISBLANK(A5)", "ISBLANK(C5)", "ISBLANK(C1)", "ISBLANK(A1)", "ISERR(1/0)", "ISERR(NA())", "ISERROR(NA())", "ISNA(NA())", "ISNA(1/0)",
 "ISLOGICAL(A6)", "ISLOGICAL(1)", "ERROR.TYPE(1/0)", "ERROR.TYPE(NA())", "ERROR.TYPE(#VALUE!)", "ISERROR(ERROR.TYPE(1))",
 # 조회
 "HLOOKUP(\"c\",E1:E5,1,FALSE)", "HLOOKUP(3,D1:D5,1)", "LOOKUP(3.5,D1:D5,E1:E5)", "LOOKUP(0,D1:D5,E1:E5)", "LOOKUP(\"c\",E1:E5,D1:D5)",
 "ROW(D3)", "COLUMN(E2)", "ROWS(A1:C7)", "COLUMNS(A1:C7)", "ROWS({1,2;3,4;5,6})",
 # 문자
 "CONCATENATE(\"A\",1,TRUE)", "_xlfn.CONCAT(E1:E5)", "_xlfn.CONCAT(\"x\",D1:D3)",
 "_xlfn.TEXTJOIN(\"-\",TRUE,\"X\",\"\",\"Y\")", "_xlfn.TEXTJOIN(\"-\",FALSE,\"X\",\"\",\"Y\")", "_xlfn.TEXTJOIN(\", \",TRUE,C1:C5)",
 "UPPER(\"hello 한글\")", "LOWER(\"WORLD\")", "REPLACE(\"abcdef\",3,2,\"XX\")", "REPLACE(\"가나다라\",2,1,\"ㅋ\")", "REPLACE(\"abc\",10,1,\"Z\")",
 "SEARCH(\"B*D\",\"aBcDe\")", "SEARCH(\"?c\",\"abcd\")", "SEARCH(\"z\",\"abc\")", "SEARCH(\"~*\",\"a*b\")", "SEARCH(\"a\",\"banana\",3)",
 "VALUE(\"1234.5\")", "VALUE(\" 12 \")", "VALUE(\"1,234\")", "VALUE(\"abc\")", "VALUE(\"50%\")", "VALUE(\"-1E3\")",
 "FIXED(1234.567,1)", "FIXED(1234.567,-1)", "FIXED(-1234.567,2,TRUE)", "FIXED(0.5,0)",
 "N(A6)", "N(\"7\")", "N(A1)", "T(C1)", "T(A1)",
 "EXACT(\"case\",\"CASE\")", "EXACT(\"a\",\"a\")", "CHAR(65)", "CHAR(97)", "CODE(\"A\")", "CODE(\"abc\")",
 # 날짜·시간
 "DATE(2026,11,12)", "DATE(2026,13,1)", "DATE(2026,1,0)", "DATE(1900,2,29)", "DATE(99,1,1)", "DATE(2024,2,30)",
 "YEAR(46338)", "MONTH(46338)", "DAY(46338)", "YEAR(60)", "DAY(60)", "MONTH(61)",
 "HOUR(0.75)", "MINUTE(0.7604166667)", "SECOND(0.5000115741)", "HOUR(46338.25)",
 "TIME(12,30,0)", "TIME(25,0,0)", "TIME(0,90,0)",
 "WEEKNUM(46338)", "WEEKNUM(46338,2)", "WEEKNUM(46023)", "WEEKNUM(46023,2)",
 "_xlfn.DAYS(46340,46338)", "_xlfn.DAYS(46338,46340)",
 "NETWORKDAYS.INTL(46332,46345)", "NETWORKDAYS.INTL(46332,46345,7)", "NETWORKDAYS.INTL(46332,46345,\"0000011\")", "NETWORKDAYS.INTL(46332,46345,11)",
 "WORKDAY(46332,1)", "WORKDAY(46332,-1)", "WORKDAY(46332,10)", "WORKDAY(46333,0)",
 "WORKDAY.INTL(46332,1,7)", "WORKDAY.INTL(46332,5,\"0000011\")",
 "NETWORKDAYS(46332,46345,{46335,46336})", "NETWORKDAYS(46345,46332)", "WORKDAY(46332,5,{46335})",
 "NETWORKDAYS.INTL(46332,46345,1,{46337})", "WORKDAY.INTL(46332,-3,\"1000001\")", "NETWORKDAYS.INTL(46332,46345,\"1111111\")",
 "NETWORKDAYS.INTL(46332,46345,8)", "VALUE(\"2026-11-12\")", "DATEVALUE(\"2026-13-01\")", "DATEVALUE(\"2025-2-29\")",
 "CEILING(4.3,0.1)", "FLOOR(4.3,0.1)", "_xlfn.CEILING.MATH(4.3,0.1)", "ROUNDUP(4.3*3,1)", "FIXED(1234.5,-2)", "FIXED(-0.5,0)",
 "DATEVALUE(\"2026-11-12\")", "DATEVALUE(\"2024/2/29\")", "DATEVALUE(\"abc\")",
]

wb = openpyxl.Workbook()
ws = wb.active
ws.title = "P1b"
for a, v in DATA.items():
    if v is not None:
        ws[a] = v
cells = []
for i, f in enumerate(F):
    addr = f"H{i+1}"
    ws[addr] = "=" + f
    ws[f"I{i+1}"] = f   # 수식 원문(글자) — 사람이 볼 때 참고
    cells.append(addr)

tmp = tempfile.mkdtemp()
src = os.path.join(OUT, "p1b_corpus.xlsx")
wb.save(src)
# LibreOffice 로 다시 계산해 저장
subprocess.run(["soffice", "--headless", "--calc", "--convert-to", "xlsx:Calc MS Excel 2007 XML",
                "--outdir", tmp, src], check=True, capture_output=True, timeout=180)
calc = openpyxl.load_workbook(os.path.join(tmp, "p1b_corpus.xlsx"), data_only=True)["P1b"]
gold = {}
for a in cells:
    v = calc[a].value
    if isinstance(v, datetime.datetime):
        v = v.isoformat()
    elif isinstance(v, datetime.time):
        v = (v.hour * 3600 + v.minute * 60 + v.second) / 86400
    gold[a] = v
# LibreOffice 가 엑셀과 다르게 계산하는 칸 — 엑셀 동작으로 덮어쓴다(근거는 Microsoft 함수 도움말).
EXCEL_OVERRIDES = {
    'PRODUCT("2",3)': 6,                 # 인수로 직접 쓴 숫자 글자는 숫자로 바꾼다(LO: #VALUE!)
    "POWER(-8,1/3)": "#NUM!",            # 음수의 분수 거듭제곱(LO: -2)
    "SQRT(-1)": "#NUM!",                 # (LO: #VALUE!)
    "CEILING(4.2,-1)": "#NUM!",          # 양수에 음수 기준(LO: #VALUE!)
    "MROUND(11,-5)": "#NUM!",            # 부호가 다르면 #NUM!(LO: 10)
    "LN(0)": "#NUM!",                    # (LO: #VALUE!)
    'CONCATENATE("A",1,TRUE)': "A1TRUE", # 논리값은 TRUE 글자(LO: A11)
    "T(A1)": "",                         # 숫자 칸은 빈 글자(LO 파일엔 빈 칸으로 저장됨)
    "DATE(1900,2,29)": 60,               # 1900 윤년 버그(LO: 1900-03-01)
    "DAY(60)": 29,                       # 같은 이유(LO: 28)
    'NETWORKDAYS.INTL(46332,46345,"1111111")': "#VALUE!",  # 전부 휴일인 주말 문자열(LO: 0)
    "NETWORKDAYS.INTL(46332,46345,8)": "#NUM!",            # 없는 주말 번호(LO: #VALUE!)
    'DATEVALUE("2024/2/29")': 45351,     # 엑셀은 / 구분 날짜를 받는다(LO 로캘: #VALUE!)
}
for a in cells:
    f = ws[f"I{int(a[1:])}"].value
    if f in EXCEL_OVERRIDES:
        gold[a] = EXCEL_OVERRIDES[f]
json.dump({"P1b": gold}, open(os.path.join(OUT, "p1b_golden.json"), "w"), ensure_ascii=False, indent=1)
shutil.rmtree(tmp)
print(len(cells), "cells")
