# Hackathon API 문서

## 개요

Hackathon API는 해커톤 프로젝트 등록 및 조회를 위한 API입니다.

- **팀 기반 구조**: 1-3명의 팀 멤버로 구성
- **GitHub 자동 연동**: 프로젝트 및 멤버 GitHub 정보 자동 fetch
- **Stale Data 자동 갱신**: 1시간 이상 된 데이터는 조회 시 자동 refresh

---

## DB 스키마

### hackathon (화이트리스트)
```sql
CREATE TABLE hackathon (
    token_id TEXT PRIMARY KEY REFERENCES token(token_id),
    created_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT
);
```

### hackathon_team (팀 정보)
```sql
CREATE TABLE hackathon_team (
    id BIGINT PRIMARY KEY,                  -- Snowflake ID (자동 생성)
    name TEXT NOT NULL,                     -- 팀 이름
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);
```

### hackathon_team_member (팀 멤버)
```sql
CREATE TABLE hackathon_team_member (
    id BIGINT PRIMARY KEY,                  -- Snowflake ID (자동 생성)
    team_id BIGINT NOT NULL REFERENCES hackathon_team(id) ON DELETE CASCADE,
    email TEXT NOT NULL,                    -- 이메일 (필수)
    discord TEXT,                           -- 디스코드
    github_username TEXT,                   -- GitHub 사용자명 (통계 자동 fetch)
    twitter TEXT,                           -- 트위터
    linkedin TEXT,                          -- 링크드인 URL
    -- GitHub 통계 (자동 fetch)
    github_image_uri TEXT,
    github_name TEXT,
    github_url TEXT,
    github_follower_count INTEGER,
    github_following_count INTEGER,
    github_repo_count INTEGER,
    github_star_count INTEGER,
    github_bio TEXT,
    github_fetched_at BIGINT,               -- 마지막 fetch 시간
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT unique_team_email UNIQUE (team_id, email)
);
```

### hackathon_project (프로젝트 정보)
```sql
CREATE TABLE hackathon_project (
    token_id TEXT PRIMARY KEY REFERENCES hackathon(token_id) ON DELETE CASCADE,
    team_id BIGINT NOT NULL REFERENCES hackathon_team(id),
    name TEXT NOT NULL,                     -- 프로젝트 이름
    description TEXT NOT NULL,              -- 프로젝트 설명
    monad_integration TEXT NOT NULL,        -- Monad 통합 설명
    github_url TEXT NOT NULL,               -- 프로젝트 GitHub URL
    demo_video_url TEXT NOT NULL,           -- 데모 비디오 URL
    agent_moltbook_url TEXT,                -- Agent Moltbook 링크 (옵션)
    -- GitHub repo 통계 (자동 fetch)
    github_star_count INTEGER DEFAULT 0,
    github_fork_count INTEGER DEFAULT 0,
    github_description TEXT,
    github_topics TEXT,                     -- 쉼표 구분
    github_language TEXT,
    github_fetched_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);
```

---

## API 엔드포인트

### 1. 해커톤 프로젝트 등록 (`POST /cms/hackathon/register`)

해커톤 프로젝트를 등록합니다. **Admin 권한 필요**

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 필수 (Admin only)

#### Request Body
```json
{
  "token_id": "0x1234567890abcdef...",
  "team_name": "Awesome Team",
  "members": [
    {
      "email": "member1@example.com",
      "discord": "member1#1234",
      "github_username": "member1",
      "twitter": "@member1",
      "linkedin": "https://linkedin.com/in/member1"
    },
    {
      "email": "member2@example.com",
      "github_username": "member2"
    }
  ],
  "project_name": "My Awesome Project",
  "project_description": "A decentralized application for...",
  "monad_integration": "Uses Monad for high-speed transaction processing...",
  "project_github_url": "https://github.com/owner/repo",
  "demo_video_url": "https://youtube.com/watch?v=xxxxx",
  "agent_moltbook_url": "https://moltbook.com/agent/xxx"
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |
| `team_name` | string | O | 팀 이름 |
| `members` | array | O | 팀 멤버 (1-3명) |
| `members[].email` | string | O | 멤버 이메일 |
| `members[].discord` | string | X | 디스코드 핸들 |
| `members[].github_username` | string | X | GitHub 사용자명 (통계 자동 fetch) |
| `members[].twitter` | string | X | 트위터 핸들 |
| `members[].linkedin` | string | X | 링크드인 URL (`https://`로 시작) |
| `project_name` | string | O | 프로젝트 이름 |
| `project_description` | string | O | 프로젝트 설명 |
| `monad_integration` | string | O | Monad 통합 설명 |
| `project_github_url` | string | O | 프로젝트 GitHub URL (`https://github.com/`으로 시작) |
| `demo_video_url` | string | O | 데모 비디오 URL (`https://`로 시작) |
| `agent_moltbook_url` | string | X | Agent Moltbook 링크 |

#### Validation Rules
- `members`: 최소 1명, 최대 3명
- `email`: 필수, `@`와 `.` 포함
- `email`: 팀 내 중복 불가
- `github_username`: 팀 내 중복 불가
- `linkedin`: `https://`로 시작
- `token_id`: 이미 등록된 경우 에러 (재등록 불가)

#### 처리 과정

1. Admin 권한 확인
2. Request validation
3. `token_id`가 `token` 테이블에 존재하는지 확인
4. GitHub API 병렬 호출:
   - Project GitHub info (stars, forks, topics, language)
   - Members GitHub info (avatar, followers, repos, stars)
5. 트랜잭션으로 DB Insert:
   - `hackathon` 테이블 (화이트리스트)
   - `hackathon_team` 테이블
   - `hackathon_team_member` 테이블 (각 멤버)
   - `hackathon_project` 테이블

#### 응답
```json
{
  "success": true,
  "token_id": "0x1234567890abcdef...",
  "team_id": "7654321098765432100",
  "github_fetch_pending": false
}
```

| 필드 | 타입 | 설명 |
|------|------|------|
| `success` | boolean | 성공 여부 |
| `token_id` | string | 등록된 토큰 ID |
| `team_id` | string | 생성된 팀 ID (Snowflake) |
| `github_fetch_pending` | boolean | GitHub 정보 fetch 실패로 pending 상태인지 |

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 token_id, validation 실패, token 미존재, 이미 등록됨)
- `401`: 인증 실패 (Admin 권한 없음)
- `500`: 내부 서버 에러 (DB 에러)

---

### 2. 해커톤 토큰 목록 조회 (`GET /token/hackathon`)

등록된 모든 해커톤 토큰 ID 목록을 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 응답
```json
{
  "token_ids": [
    "0x1234567890abcdef...",
    "0x5678901234abcdef...",
    "0xabcdef1234567890..."
  ]
}
```

- `created_at` 내림차순 정렬 (최신순)

---

### 3. 해커톤 토큰 상세 조회 (기존 API 활용)

`GET /token/:token_id` API를 사용하면 `hackathon_info` 필드에 해커톤 정보가 포함됩니다.

#### Stale Data 자동 갱신
- `github_fetched_at`이 1시간 이상 지난 경우 자동으로 GitHub API 재호출
- Project 및 Member 모두 갱신

#### 응답 예시 (hackathon_info 부분)
```json
{
  "token_info": {
    "token_id": "0x...",
    "name": "Token Name",
    "hackathon_info": {
      "team": {
        "id": "7654321098765432100",
        "name": "Awesome Team",
        "members": [
          {
            "email": "member1@example.com",
            "discord": "member1#1234",
            "twitter": "@member1",
            "linkedin": "https://linkedin.com/in/member1",
            "github": {
              "username": "member1",
              "image_uri": "https://avatars.githubusercontent.com/...",
              "name": "Member One",
              "url": "https://github.com/member1",
              "follower_count": 100,
              "following_count": 50,
              "repo_count": 30,
              "star_count": 500,
              "bio": "Developer",
              "fetch_pending": false
            }
          },
          {
            "email": "member2@example.com",
            "discord": null,
            "twitter": null,
            "linkedin": null,
            "github": {
              "username": "member2",
              "image_uri": "https://avatars.githubusercontent.com/...",
              "name": "Member Two",
              "url": "https://github.com/member2",
              "follower_count": 50,
              "following_count": 20,
              "repo_count": 15,
              "star_count": 100,
              "bio": null,
              "fetch_pending": false
            }
          }
        ]
      },
      "project": {
        "name": "My Project",
        "description": "Project description",
        "monad_integration": "Uses Monad for...",
        "github_url": "https://github.com/owner/repo",
        "demo_video_url": "https://youtube.com/...",
        "agent_moltbook_url": "https://moltbook.com/...",
        "github_star_count": 150,
        "github_fork_count": 30,
        "github_description": "A great project",
        "github_topics": ["blockchain", "web3", "monad"],
        "github_language": "TypeScript"
      }
    }
  }
}
```

---

## TypeScript Interfaces

### 요청 타입

```typescript
// 팀 멤버 입력
interface TeamMemberInput {
  email: string;              // 필수
  discord?: string;
  github_username?: string;   // 있으면 GitHub 통계 자동 fetch
  twitter?: string;
  linkedin?: string;          // https://로 시작
}

// POST /cms/hackathon/register
interface RegisterHackathonRequest {
  token_id: string;
  team_name: string;
  members: TeamMemberInput[];  // 1-3명
  project_name: string;
  project_description: string;
  monad_integration: string;
  project_github_url: string;  // https://github.com/으로 시작
  demo_video_url: string;      // https://로 시작
  agent_moltbook_url?: string;
}
```

### 응답 타입

```typescript
// POST /cms/hackathon/register
interface RegisterHackathonResponse {
  success: boolean;
  token_id: string;
  team_id: string;
  github_fetch_pending: boolean;
}

// GET /token/hackathon
interface HackathonTokenListResponse {
  token_ids: string[];
}

// HackathonInfo (TokenInfo에 포함)
interface HackathonInfo {
  team: HackathonTeamInfo;
  project: HackathonProjectInfo;
}

interface HackathonTeamInfo {
  id: string;
  name: string;
  members: HackathonTeamMemberInfo[];
}

interface HackathonTeamMemberInfo {
  email: string;
  discord?: string;
  twitter?: string;
  linkedin?: string;
  github?: HackathonMemberGitHubInfo;
}

interface HackathonMemberGitHubInfo {
  username: string;
  image_uri: string;
  name: string;
  url: string;
  follower_count: number;
  following_count: number;
  repo_count: number;
  star_count: number;
  bio?: string;
  fetch_pending: boolean;  // true if stats not fetched yet
}

interface HackathonProjectInfo {
  name: string;
  description: string;
  monad_integration: string;
  github_url: string;
  demo_video_url: string;
  agent_moltbook_url?: string;
  github_star_count: number;
  github_fork_count: number;
  github_description?: string;
  github_topics?: string[];
  github_language?: string;
}
```

---

## GitHub API 연동

### 사용되는 GitHub API

1. **User API**: `GET https://api.github.com/users/{username}`
   - 프로필 정보 (avatar, name, followers, following, public_repos, bio)

2. **User Repos API**: `GET https://api.github.com/users/{username}/repos`
   - 총 스타 수 계산 (모든 레포의 stargazers_count 합계)

3. **Repository API**: `GET https://api.github.com/repos/{owner}/{repo}`
   - 프로젝트 정보 (stars, forks, topics, language, description)

### Rate Limit

- **인증 없음**: 60 requests/hour
- **Personal Access Token**: 5,000 requests/hour

현재 환경변수 `GITHUB_TOKEN`에 설정된 토큰으로 인증하여 5,000 requests/hour 사용 중.

### 자동 갱신

- **HACKATHON_REFRESH_INTERVAL_SECS**: 기본 3600초 (1시간)
- 조회 시 `github_fetched_at`이 기준보다 오래되면 자동 갱신
- Project와 Members 모두 갱신

---

## 환경변수

```env
# GitHub API Token (필수)
GITHUB_TOKEN=ghp_xxxxxxxxxxxxx

# GitHub 데이터 갱신 주기 (초, 기본: 3600)
HACKATHON_REFRESH_INTERVAL_SECS=3600
```
