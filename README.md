# 방과후 통합 매니저

학교 방과후학교의 **수강생 · 수익자 부담금 · 방과후 이용권 · 자유수강권 · 정산 · 품의**를
한 프로그램에서 관리합니다. 개인정보를 다루므로 **외부 서버나 클라우드를 쓰지 않고
모든 자료를 이 컴퓨터에만** 저장합니다.

- 설계 문서: [docs/01-설계안.md](docs/01-설계안.md)
- 지금 상태: **Phase 1 완료** (기초 데이터 · Excel · 화면 뼈대)

## 지금 되는 것 (Phase 1)

| 화면 | 내용 |
|---|---|
| 대시보드 | 전교생 · 지원대상 · 부서 수와 준비 순서 |
| 작업공간 | 만들기 · 수정 · 삭제 · 현재 작업공간 선택. **시작일 순 자동 정렬**, 기간 겹침 경고 |
| 학생정보 | 전교생 명단, 학년·반·지원유형 필터, 검색, 정렬, 선택/전체 삭제, Excel |
| 지원대상자 | 이용권 · 자유수강권 명단, **적용 기간**, 대상학년 불일치 경고, Excel |
| 부서정보 | 부서 · 반 · 강사 · 요일 · 항목별 기준 수강료, Excel |
| Excel | 업로드 양식 받기 · 업로드(미리보기 검증) · 내려받기 · 오류 목록 내려받기 |

수강생 명단(Phase 2), 정산 엔진(Phase 3), 품의자료(Phase 4), 배포(Phase 5)는 아직입니다.
Sidebar에 어느 Phase에서 만드는지 표시해 두었습니다.

## 개발

```bash
npm install
npm run app        # 개발 중 실행 (창이 뜹니다)
npm run check      # 타입 검사 + Rust 단위 테스트
npm run app:build  # 설치 파일 만들기
```

| 명령 | 하는 일 |
|---|---|
| `npm run dev` | 화면만 브라우저에서 (Tauri 명령은 동작하지 않습니다) |
| `npm run build` | 타입 검사 + 화면 번들 |
| `npm run check` | `tsc --noEmit` + `cargo test` |
| `npm run version:set 0.2.0` | 버전 세 곳을 한꺼번에 맞춤 |

## 구조

```
src/                      화면 (React + TypeScript)
  components/             공용 UI — 표 · 팝업 · 토스트 · Excel 도구
  pages/                  화면별 구현
  ipc/                    Rust 명령 호출 (api.ts) 와 타입 (types.ts)
  lib/                    포맷터 · 전역 상태
  styles/                 디자인 토큰과 공용 스타일
src-tauri/
  migrations/001_init.sql 스키마 원본 — 배포된 파일은 고치지 않는다
  src/db/                 연결 · 마이그레이션
  src/repo/               SQL
  src/domain/             업무 규칙 (DB 의존 없음, 테스트 대상)
  src/excel/              읽기(calamine) · 쓰기(rust_xlsxwriter)
  src/commands/           IPC 껍데기 (로직 없음)
```

## 자료가 저장되는 곳

| 대상 | 위치 |
|---|---|
| 업무 DB | `%APPDATA%\kr.school.afterschool\afterschool.db` |
| 백업 | 위 폴더의 `backups\` |
| Excel 출력 | 위 폴더의 `exports\` |

실행 파일과 자료를 나누어 두었으므로 **프로그램을 업데이트해도 자료는 그대로**입니다.
스키마가 바뀌는 업데이트에서는 적용 직전에 DB 파일을 `backups\`에 자동 복사합니다.

## 남은 일

- [ ] 앱 아이콘 — 지금은 자리표시용입니다. Phase 5에서 교체합니다.
- [ ] Phase 2 수강 관리, Phase 3 정산 엔진, Phase 4 품의, Phase 5 배포
