# imauth 속도·자원 사용·안정성 진단

조사일: 2026-09-08 KST. 대상: 현재 저장소와 로컬 Docker에서 실행 중인 imreader의 imauth 0.7.1 스택.

**우선순위는 CDP 중계의 10초 유휴 끊김 수정 → 요청 대기·취소·복구 개선 → 브라우저 상시 실행 비용 제거다.** 서버 프레임워크 교체를 우선할 근거는 확인되지 않았다.

Aside CLI 세션 `bk1m4MNDy3se4cRo`로 공식 문서·upstream source를 조사했고, 별도로 실제 컨테이너의 자원 사용, SDK 읽기 요청, CDP 연결을 측정했다. 이 문서는 진단과 수정 제안이다. 서비스 코드·설정·이미지·저장된 인증 상태는 변경하지 않았다.

## 1. 직접 측정한 결과

### 자원 사용

로그인 작업과 viewer 조작을 시작하지 않은 상태에서 `docker stats --no-stream`을 두 번 실행했다.

| 구성 요소 | 메모리, 첫 관측 / 두 번째 관측 | CPU, 첫 관측 / 두 번째 관측 | 로컬 이미지 크기 |
| --- | --- | --- | --- |
| Rust gRPC 서버 | 11.34 / 17.14 MiB | 0.00 / 0.00% | 42,791,164 bytes |
| Chromium + KasmVNC 데스크톱 | 645.5 / 646.8 MiB | 0.07 / 0.33% | 1,131,537,229 bytes |
| viewer proxy | 13.34 / 13.34 MiB | 0.00 / 0.00% | 26,192,672 bytes |

브라우저 컨테이너가 세 컨테이너 메모리 합계의 약 96%를 차지했다. `ps`에서 Chromium, Xvnc, xfce4-session, xfwm4, xfdesktop 등이 실제 실행 중이었다. KasmVNC는 `1.4.0.663b6d6a0bdd4638bff981c75a522056aaaa1c2e`, 컨테이너 이미지는 모두 arm64였다.

이는 두 시점의 컨테이너 메모리 관측이다. 평균·최댓값·메모리 누수 측정이 아니며, 프로세스별 RSS를 합산한 값도 아니다. 이미지 크기는 로컬 Docker image inspect 값으로, registry 압축 전송량과 구별해야 한다. `shm_size: 2gb`는 이 관측에서 2 GiB를 실제 사용했다는 뜻이 아니다.

### 실제 읽기 API

실행 중인 imreader의 `/app/.venv/bin/python`과 설치된 `ImauthClient`로, 기존 런타임 설정을 사용해 `get_connection_status()`를 호출했다. 키와 주소 값은 출력하지 않았다.

| 항목 | 결과 |
| --- | --- |
| 첫 호출 | 8.204 ms |
| 동일 클라이언트 후속 20회 p50 | 0.424 ms |
| 후속 20회 p95, nearest rank | 0.491 ms |
| 후속 20회 최대 | 0.515 ms |
| 응답 | 5개 플랫폼, 연결됨 0개 |

같은 호스트의 Docker 네트워크에서 저장 쿠키가 없는 상태를 읽은 소규모 측정이다. 실제 로그인 지연이나 원격 네트워크 p95를 대신하지 않는다. 현재 증거는 단순 gRPC/SQLite 조회 경로가 주된 병목이라는 가설을 지지하지 않는다.

### CDP 연결 끊김: 재현 및 원인 분리

동일한 Chrome의 browser WebSocket에 연결해 읽기 전용 `Browser.getVersion` 성공을 확인한 후, 명령을 보내지 않고 기다렸다. 쿠키·페이지·로그인 상태를 변경하는 CDP 명령은 보내지 않았다.

| 연결 경로 | 유휴 관측 | 이후 Browser.getVersion |
| --- | --- | --- |
| Chrome 직접 연결, 127.0.0.1:9222 | 14.004초 후에도 열림 | 성공 |
| 현재 서비스의 중계, 컨테이너 주소:9223 | **10.016초에 EOF** | 실패, EOFError |
| 실행 중인 스크립트에서 추출한 원본 중계의 격리 실행 | **10.014초에 EOF** | 실패 |
| 같은 격리 중계에 `upstream.settimeout(None)` 한 줄 추가 | 14.015초 후에도 열림 | 성공 |

격리 실행은 컨테이너 내부 임시 loopback 포트와 메모리 내 코드로 수행했다. 기존 중계 프로세스·파일은 변경하지 않았고, 실험 프로세스와 연결은 종료됐다.

실행 중인 `/chrome-entrypoint.sh`와 저장소 `scripts/chrome-entrypoint.sh`의 SHA-256이 모두 아래 값으로 일치했다.

```text
7a4d93d6a53fa37fbd77a29f39b5380aff8f33ed66d776f9a436a7ba7e4d4ca8
```

**원인:** [중계 스크립트](../scripts/chrome-entrypoint.sh)의 36행에서 지정한 연결 타임아웃이 연결 성립 후에도 upstream 소켓에 남는다. `pump()`의 읽기가 약 10초 동안 데이터를 받지 못하면 예외를 삼키고 양방향 연결을 shutdown한다. 연결 단계의 제한과 장기 WebSocket 전송 수명을 분리해야 한다. Python의 `create_connection` 및 `settimeout(None)` 동작은 [공식 socket 문서](https://docs.python.org/3/library/socket.html#socket.create_connection)와도 일치한다.

제안하는 최소 변경은 다음과 같다. **아래 변경은 실제 서비스나 저장소 코드에 적용하지 않았다.**

```diff
 upstream = socket.create_connection(("127.0.0.1", 9222), timeout=10)
+upstream.settimeout(None)
```

이 실험은 10초 유휴 끊김의 원인을 확인한다. 장시간 안정성이나 실제 provider 로그인 성공을 입증하지 않는다. 정상 cookie polling처럼 응답이 계속 오면 이 타임아웃 조건에 도달하지 않을 수 있다. 확인한 최근 24시간의 보존된 서버 로그에는 cookie poll error, reconnect, WebSocket reset, timeout, WARN, ERROR가 모두 0건이었다. 과거 사용자 증상을 전부 이 원인으로 단정하지 않는다.

## 2. 수정 우선순위

| 순서 | 변경 | 현재 근거 | 기대 효과와 검증 경계 |
| --- | --- | --- | --- |
| 1 | CDP 연결 성립 후 소켓 타임아웃 해제 | 위 실제 A/B 실험 | 10초 유휴 단절 제거를 14초 실험으로 확인. 장기 연결·종료 정리·실제 로그인은 후속 검증 필요 |
| 2 | 슬롯 대기, CDP 연결, 사용자 입력의 시간 예산 분리; 모든 장기 await에 취소·전체 상한 적용 | `application/container.rs:61`, `application/login.rs:97`, `:145`, `:185`, `browser_factory.rs:102` | 사용자가 취소했는데 수십 초~수분 동안 작업이 남는 구조 개선. 현재 사용자 체감 지연은 재현하지 않음 |
| 3 | CDP handler 종료를 즉시 전달하고 재연결 시 기존 slot·target 소유권 유지 | `browser_factory.rs:60`, `application/login.rs:175`, `:243` | 오류 3회 누적 후 복구하는 정책과 매번 새 페이지를 만드는 경로의 개선 후보. 지연 감소량은 미측정이며 현재 page 1개라 실제 누적은 관측되지 않음 |
| 4 | 브라우저를 로그인 때 시작하고, 활성 로그인·복구가 없으면 유휴 종료 | Chrome/Kasm 메모리 약 646 MiB, `docker-compose.yml` 상시 구동 | 상시 메모리 비용을 줄이는 가장 큰 구조적 후보. 시작 지연·로그인 상태 복원·경합 검증 필요; 절감량 미측정 |
| 5 | 요청 수 제한과 바쁜 상태 응답을 명시; 여러 slot 사용 시 먼저 비는 slot 선택 | `grpc.rs:127` 요청마다 spawn, `browser_factory.rs:136` 첫 slot만 대기 | 과도한 대기 작업과 다중 slot의 불필요한 대기 감소. 현재 한 slot 배포에서 동시 로그인 부하 실험은 하지 않음 |
| 6 | 쿠키 영속화 성공을 확인한 뒤 로그인 성공 확정 | `application/login.rs:302` 이후 save 실패를 warn만 남기고 Connected 유지 | 저장 실패 뒤 성공처럼 보이는 상태 방지. 저장 장애를 주입하거나 실제 상태를 변경하지 않았으므로 코드상 후보 |

### 시간 제한과 취소의 구체적인 문제

- 기본 `login_timeout_secs=300`을 `PooledBrowserFactory`의 semaphore 대기와 CDP 연결에도 각각 전달한다. 따라서 300초가 요청 전체 상한이 아니다.
- 사용자 입력 deadline은 최초 acquire, 페이지 생성, navigation이 끝난 뒤 시작한다. polling 안의 sleep, cookie read, reconnect도 이 deadline을 독립적으로 초과할 수 있다.
- receiver 종료 확인은 acquire·페이지 생성 앞뒤에 있고, 저장소의 세션 삭제 확인은 cookie polling에서 이뤄진다. 진행 중인 장기 await를 즉시 중단시키는 취소 신호가 없다.
- 슬롯별 semaphore는 이미 있어서 활성 로그인 동시성을 제한한다. 하지만 요청마다 별도 task를 만드는 경로 전체의 admission limit과 동일하지 않다. 이벤트용 `mpsc::channel(10)`도 전체 로그인 요청 큐 크기 제한은 아니다.
- `BrowserConfig.max_pool_size`와 `page_timeout_secs`는 선언·접근자·테스트에는 있지만 현재 실행 경로가 소비하지 않는다. 실제 pool 크기는 CDP URL 수이며 navigation은 `30`을 직접 전달한다. 설정값만 조절하는 튜닝은 효과가 없다.

개선 시 `tokio::select!`, 요청별 취소 신호와 [timeout_at](https://docs.rs/tokio/latest/tokio/time/fn.timeout_at.html)를 조합하되, future를 drop하는 것과 외부 Chrome 작업·탭을 정리하는 것은 구별해야 한다. 종료 처리에도 상한을 두고 slot/target을 정확히 회수해야 한다.

### 재연결의 구체적인 문제

현재 handler는 실제로 계속 poll되고 있다. 문제는 handler가 종료되어도 그 상태를 로그인 작업에 직접 알리지 않는 점이다. cookie read 오류 3회 후에야 복구를 시도한다.

복구 시 기존 browser lease를 놓고 일반 `acquire()`를 다시 부르며 새 페이지를 만든다. 다중 slot에서는 다른 profile로 이동하거나 다른 대기 로그인과 경합할 수 있다. 기존 page를 `close()`하지 않고 새 페이지를 생성하도록 구현되어 있으며, 최초 전달한 viewer URL도 새로 전달하지 않는다. 따라서 **기존 slot 예약을 유지하며 동일 target에 재부착하는 복구**를 우선 검토해야 한다. 로그인 화면을 무작정 닫는 방식은 진행 중인 CAPTCHA/2FA 입력을 잃게 할 수 있다.

## 3. 가벼워지는 구조의 선택

기존 `GetCookies`, `ValidateSession`, `GetConnectionStatus`는 이미 브라우저를 acquire하지 않고 SQLite를 읽는다. cookie 전용 빠른 경로를 새로 만드는 것보다 **배포에서 브라우저 생명주기를 분리**하는 것이 핵심이다. Compose의 서버는 현재 Chrome healthy를 시작 조건으로 갖는다.

| 선택 | 적용 판단 | 제약 |
| --- | --- | --- |
| 기존 Kasm/Chromium을 필요한 때 실행 | 먼저 검증할 구조 변경 | active lease가 있을 때 종료 금지, profile 보존·동시 시작 중복 방지·cold start 필요. Xvnc만 끄면 화면에 붙은 Chrome도 영향을 받을 수 있음 |
| CDP 연결을 slot 수명 동안 재사용 | 재연결 문제 수정 뒤 측정 | 프로세스 시작 비용 감소와 별개. handler 감시·연결 재생성·탭 정리 없이 캐시만 추가하면 고장 난 연결을 재사용 |
| native headed Chrome, 사용자 로컬 인증용 | 로컬 전용 사용 모드의 후보 | Linux 원격 viewer를 대체하지 못함. 기존 `scripts/start-local.sh`가 유사 경로를 갖지만 이번에 실행하거나 검증하지 않음 |
| minimal headed Chromium + 작은 X 환경 | 상시 원격 조작이 필요할 때 후보 | 현재 데스크톱 구성 요소 일부를 제거 가능하나 실제 로그인·한글 입력·폰트·popup 검증 필요 |
| headless + CDP screencast viewer | 별도 실험 후 판단 | 입력·IME·popup·다운로드·권한 화면·frame ack 구현이 필요. 자동으로 Kasm과 동등해지지 않음 |
| browser/context를 여러 개로 늘리기 | 기본 해결책으로 채택하지 않음 | 프로세스 증가로 메모리가 늘며 context 격리가 화면·입력 격리를 제공하지 않음 |
| Playwright 또는 서버 언어 교체 | 현 단계 보류 | 측정된 중계·수명 관리 문제를 자동으로 해결한다는 증거 없음 |

Aside의 요청별 incognito context 제안도 기본안으로 채택하지 않았다. 현재 persistent profile의 로그인 상태 유지와 수동 viewer 조작을 바꾸므로, cookie 외 저장소 복원 및 동일 사용자 화면 유지부터 검증해야 한다. 이 기능은 10초 중계 단절을 고치는 데 필요한 조건이 아니다.

`ValidateSession`은 저장된 세션 쿠키 존재·만료 조건을 검사하며 provider 서버에 실제 요청해 유효성을 확인하지 않는다. 저장 쿠키가 있다는 이유만으로 로그인 재사용 성공을 보장하면 안 된다. 실제 보호 자원 접근 결과와 쿠키 재발급 흐름이 별도로 필요하다.

README 첫 문장의 headless login flow 설명과 달리 현재 `LoginUseCase`는 페이지를 열고 사용자 입력을 기다리는 경로다. 별도의 자동 로그인 또는 headless→headed 전환이 구현돼 있다고 전제하지 않았다.

## 4. 변경 후 통과해야 할 검증

아래는 제안하는 검증 항목이며 이번 조사에서 통과했다고 주장하는 항목이 아니다.

1. 중계 연결을 60초 이상 유휴 상태로 둔 뒤 CDP 명령 성공; 반복 연결·클라이언트 종료 뒤 소켓과 thread 수가 기준으로 복귀.
2. 실제 provider recording으로 페이지 열기, 사용자 입력, CAPTCHA/2FA, 로그인 완료, 쿠키 저장·재사용을 확인. provider를 변경하면 AGENTS.md의 재기록 요구 적용.
3. acquire 대기·navigation·cookie read·reconnect 각각에서 취소를 주입. 제안 목표는 취소 응답 1초 이내, 제한된 cleanup 시간 뒤 lease 반납. 실제 CDP에서 검증.
4. CDP 연결 강제 단절 뒤 동일 slot·target·viewer를 유지하고 복구. 반복 후 page 수 증가 없음.
5. 두 로그인 요청의 busy/queue 정책과 세 번째 초과 요청 응답을 검증. 여러 slot 구성은 첫 slot이 바쁠 때 다른 slot 해제로 진행되는지 확인.
6. 브라우저 중지 상태에서 cookie 조회 API가 동작하고, 첫 로그인으로 한 번만 브라우저가 시작되는지 확인. 활성 로그인 동안 유휴 종료 금지.
7. 같은 계정·페이지·해상도로 cold start, viewer 준비 시간, 인증 완료 인지 시간, idle/active/peak memory를 비교. 사용자 입력 시간은 시스템 대기 시간과 분리.
8. cookie 저장 실패 시 성공 이벤트를 내보내지 않고, 복구 가능한 실패 상태를 반환하는지 확인.

권장 계측: `slot_wait_ms`, `cdp_connect_ms`, `navigation_ms`, `cookie_read_ms`, `cancel_to_release_ms`, `reconnect_count`, 활성 login/target 수, 브라우저 전체 컨테이너 메모리. 로그에는 cookie·viewer token·인증정보를 포함하지 않는다.

## 5. 조사 범위와 추가 관측

- 실제 provider 로그인·viewer UI 조작·장기 부하·메모리 누수·cold start는 측정하지 않았다. 로그인 및 인증 정보 입력도 수행하지 않았다.
- 컨테이너 restartCount는 모두 0, OOMKilled는 모두 false였다. 이 상태만으로 과거 OOM이나 애플리케이션 오류가 없었다고 판단하지 않는다.
- Rust 서버는 실행 버전 0.7.1을 확인했으나 저장소 전체와 실행 바이너리의 동일 SHA는 증명하지 않았다. 중계 스크립트만 파일 해시로 동일성을 확인했다.
- `imauth --help`가 설정된 API 키 값을 출력하는 별도 문제를 실제 확인했다. 값은 이 문서에 남기지 않았다. `crates/imauth-cli/src/cli_support.rs:27`에 `hide_env_values = true` 적용을 제안한다.
- 소스 수정·전체 테스트·빌드·커밋·푸시·배포를 수행하지 않았다. 읽기 RPC 측정과 임시 중계 A/B 실험 결과를 구현 완료로 간주하지 않는다.

## 6. Aside에서 확인한 공식 참고 자료

아래 문서는 API와 설계 제약의 근거다. 이 환경에서의 성능 개선 수치를 제공하는 벤치마크가 아니다.

- [chromiumoxide 0.9.1 Browser](https://docs.rs/chromiumoxide/0.9.1/chromiumoxide/browser/struct.Browser.html): 기존 Chromium 연결과 browser/context 관련 API. 현재 Cargo.lock은 0.9.1.
- [chromiumoxide 0.9.1 Handler](https://docs.rs/chromiumoxide/0.9.1/chromiumoxide/handler/struct.Handler.html): 요청과 이벤트를 구동하는 handler의 수명 관리.
- [Chrome Headless mode](https://developer.chrome.com/docs/chromium/headless): headless와 headed 실행의 관계. 전환만으로 비용·로그인 성공률이 개선된다고 보장하지 않는다.
- [CDP Target.createBrowserContext](https://chromedevtools.github.io/devtools-protocol/tot/Target/#method-createBrowserContext): context 생성·격리 API. visible desktop 입력 격리는 별도 과제.
- [CDP Storage.getCookies](https://chromedevtools.github.io/devtools-protocol/tot/Storage/#method-getCookies): browserContextId 범위 cookie 접근. cookie만으로 모든 로그인 상태를 복원할 수 있다고 전제하지 않는다.
- [Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown): 취소 통지와 작업 종료 대기.
- [KasmVNC configuration](https://www.kasmweb.com/kasmvnc/docs/latest/configuration.html): FPS·품질·해상도·종료 관련 설정. 현재 설치된 1.4.0에서 개별 옵션 지원을 확인한 뒤 적용해야 한다.

현재 image는 이미 24 FPS, 품질 4~6, 일부 부가 서비스 비활성화 설정을 갖는다. 이 설정을 다시 제안하거나 `latest` 문서의 옵션을 설치 버전 지원 확인 없이 적용하지 않는다.
