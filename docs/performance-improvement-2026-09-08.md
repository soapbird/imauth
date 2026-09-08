# imauth 성능·안정성 개선 결과

2026-09-08. 로컬 소스 변경과 격리 환경 검증 기록이다. 기존 imreader 운영 컨테이너와 인증 데이터는 변경하지 않았다. 아래 결과는 운영 배포 전 검증이다.

## 변경 내용

- CDP relay의 연결용 10초 timeout이 장기 WebSocket 읽기에 남지 않도록 해제했다.
- Chrome 컨테이너는 작은 supervisor만 상시 실행한다. 첫 CDP 요청 때 Chromium·KasmVNC·데스크톱을 함께 시작하고, 마지막 CDP 연결 종료 후 유휴 시간이 지나면 함께 종료한다. `/healthz`는 브라우저를 깨우지 않는다.
- 브라우저 슬롯 대기, CDP 접속, 페이지 준비, 사용자 로그인 시간 예산을 분리했다. 스트림 종료와 Cancel RPC를 장기 작업 중에도 감시하고, 정리 작업에도 제한 시간을 둔다.
- 먼저 비는 슬롯을 사용하고 요청 수를 제한한다. 기본 CDP 1개와 대기 허용 8개에서는 10번째 요청을 `RESOURCE_EXHAUSTED`로 거절한다.
- 재연결은 같은 슬롯·target·viewer URL을 유지한다. 새 로그인 탭을 만들거나 기존 탭을 다시 이동하지 않는다. handler 종료를 페이지 작업에 전달하고, 중단된 연결 task와 생성 중인 target도 정리한다.
- 포함된 Chromium 라이브러리의 `fetch_targets`가 중복 attach를 만드는 경로를 피하고, `Target.getTargets` 조회 뒤 기존 target의 준비 상태를 확인한다. 탭 생성 직후 ID를 보관하여 페이지 준비 중 취소에도 `Target.closeTarget`으로 정리한다.
- 로그인 쿠키와 Connected 상태를 같은 SQLite 트랜잭션으로 저장한다. 저장 오류 또는 삭제된 세션에서는 이전 쿠키를 보존하며 롤백한다.
- CLI와 서버 도움말에서 API 키 환경변수 값이 노출되지 않도록 했다.
- README의 headless 로그인 설명을 실제 user-driven 실행 방식에 맞추고, 실제 CDP relay 포트 9223과 동작하는 설정값을 문서화했다. `max_pool_size`는 기존처럼 컨테이너 수를 제어하지 않으며 실제 풀은 CDP URL 목록을 따른다.

## 격리 컨테이너 실측

초기 runtime 검증 Chrome 후보 이미지: `sha256:874211a58cf44fb9936badf3229c12b7efbb10b1f43e5a665fa6dd6389d17458`.
유휴 종료 반복 검증에는 기본 60초 대신 3초를 사용했다. 운영 설정 기본값은 60초다.

| 항목 | 관측 결과 |
| --- | --- |
| 기존 실행 Chrome 컨테이너 메모리 | 약 646 MiB, 이전 진단의 유휴 스냅샷 |
| 새 컨테이너 최초 대기 메모리 | 6.41 MiB |
| 빈 페이지 사용 후 유휴 메모리 | 11.22 MiB |
| 실제 NAVER 페이지 사용 후 유휴 메모리 | 약 39 MiB, 파일 캐시 포함 |
| 브라우저 콜드 스타트 / 재기동 | 1.527초 / 1.198초 |
| Chromium 단독 종료 후 다음 요청에서 복구 | 2.444초 |
| CDP WebSocket 14초 유휴 후 명령 | 두 번째 `Browser.getVersion` 성공 |
| 유휴 종료 후 잔존 상태 | 데스크톱 프로세스 0, 좀비 0, 9222·6901 listener 없음 |
| 프로필 | 종료·재기동 뒤 테스트 마커 유지 |

메모리는 동일 프로필을 사용한 통제 A/B나 peak 측정이 아니다. 실제 로그인 페이지를 열면 메모리가 증가하고, 종료 후에도 파일 캐시가 일부 남는다. 이미지 기반과 이미지 용량 약 1.13GB는 줄이지 않았다.

## 실제 사용 경로

- Python SDK → 로컬 빌드 gRPC 서버 → 격리 Chrome → NAVER 로그인 페이지를 실행했다. `WaitingForUser`까지 2.345초가 걸렸고, Aside CLI의 KasmVNC 화면에서 실제 로그인 폼을 확인했다.
- Cancel RPC 뒤 0.202초에 Failed 이벤트와 빈 쿠키를 받았다. 후속 GetStatus는 없음, 탭 수는 2개에서 1개로 돌아갔다. 이후 브라우저·화면 서버가 종료됐다.
- 마지막 소스 변경 후 다시 빌드한 서버에서도 SDK → 실제 NAVER 페이지 → 취소를 확인했다. 이미 시작된 브라우저에서는 Waiting까지 0.457초, Cancel → Final은 0.260초였다. HTTP 탭 목록은 Final 직후 잠시 2개였고 0.070초 뒤 1개로 복귀했다. 닫기 명령 응답과 외부 목록 반영은 동시에 일어나지 않는다.
- 별도 합성 쿠키 canary에서는 Connected 이벤트, GetStatus, GetCookies, SQLite 암호화 저장 상태가 일치했다. 이는 쿠키 감지·저장 경로 검증이며 실제 NAVER 계정 인증 성공을 뜻하지 않는다.
- 새 서버의 상태 조회 20회 측정은 p50 0.360ms, p95 0.802ms였다. 기존 측정과 실행 위치가 달라 성능 개선율로 비교하지 않는다.

## 검증 기록

- `cargo build -p imauth-server -p imauth-cli`: 통과.
- `cargo test --workspace`: 177개 통과. 기본 검사에서 제외하는 실제 CDP 검사 1개는 격리 Chromium에서 별도로 실행했다.
- 최종 adapter로 실제 CDP 검사 3회 연속 통과 후, 별도 컨테이너에서 추가 1회 통과했다. 14초 유휴 후 명령, 같은 target과 DOM 상태 유지, close·생성 중 취소·Drop 뒤 탭 수 복귀를 확인했다.
- Chrome runtime 회귀 검사 5개, Python SDK 64개, TypeScript SDK 31개 통과.
- `make quality`, `cargo clippy --workspace --all-targets -- -D warnings`, 변경 Rust 파일 LSP 오류 검사, `git diff --check`: 통과.
- 도움말과 잘못된 CLI 입력을 실제 실행하여 종료 코드 및 합성 API 키 값 미노출을 확인했다.

상세 증거는 다음 로컬 경로에 있다.

- `.omo/evidence/performance-2026-09-08/`: 빌드·전체 검사, SDK 실행, Aside 화면, 취소·합성 canary
- `.omo/evidence/chrome-demand-runtime-20260908/verification.md`: Chrome 실제 실행 및 자원 수명주기
- `.omo/evidence/cdp-session-recovery/`: 같은 target 재접속과 정리 검증
- `.omo/evidence/atomic_login_persistence/2026-09-08.md`: 실제 SQLite 롤백 검사 5건
- `.omo/evidence/login-lifecycle-2026-09-08.md`: 실제 tonic 대기열·취소 검사
- `.omo/evidence/cli-secret-help-2026-09-08.md`: 도움말 노출 실패 재현과 수정 검증

검증 후 전용 서버 프로세스, Chrome 컨테이너 2개, 프로필 볼륨 2개와 임시 DB를 제거했다. 로컬 후보 이미지와 검증 증거는 남겼다. 기존 imreader의 imauth 서버·Chrome·viewer는 모두 실행 중이며 restart 0, OOM false를 다시 확인했다.

## 추가 실계정 검증

- Chromium 어댑터의 불필요한 Arc·Box·복제와 단일 호출 generic helper를 제거했다. 동작을 유지하며 순 16줄 감소했고, 전체 Rust 검사와 별도 실제 CDP 검사를 다시 통과했다.
- 한국어 리소스를 포함한 Chrome 후보와 현재 빌드 서버에서 사용자가 노벨피아에 직접 로그인했다. SDK가 `Connected`를 받았고, 후속 GetStatus·GetCookies·GetConnectionStatus가 일치했다. 저장된 쿠키 29개는 모두 암호화되어 있었다.
- 같은 DB와 키로 서버를 재시작한 뒤 연결 상태와 쿠키를 다시 읽었다. 저장 쿠키로 노벨피아에 GET 요청했을 때 회원으로 인식됐으며, 같은 주소의 무쿠키 요청은 비회원으로 인식됐다. 응답은 모두 HTTP 200이었다. 쿠키 값과 회원 식별자는 기록하지 않았다.
- 초기 두 시도는 브라우저 프로세스 교체와 CDP reset으로 실패했다. 격리 컨테이너에서 `oom_kill=2`, 메모리 peak 약 6.93 GiB를 관측했다. 이후 Chrome 뷰어로 진행한 시도는 로그인까지 유지됐고 OOM 횟수가 증가하지 않았다. 최초 실패의 정확한 유발 원인과 장시간 안정성은 확인되지 않았다.
- 상세 증거: `.omo/evidence/novelpia-real-login-2026-09-08/`의 `result.json`, `reuse-result.json`, `memory-observation.log`.

CAPTCHA·2FA 완료 여부, 장시간 부하, 운영 배포 후 개선율은 검증하지 않았다. provider별 URL·도메인·쿠키 판정 규칙은 변경하지 않았다.
