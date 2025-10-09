# Hilo: Social Pairing Event Backend - Project Contigo

[![Dev CI](https://github.com/Mapleshade20/hilo/actions/workflows/ci.yml/badge.svg?branch=dev)](https://github.com/Mapleshade20/hilo/actions)
[![Release](https://img.shields.io/github/v/release/Mapleshade20/hilo?logo=rocket)](https://github.com/Mapleshade20/hilo/releases)
[![Rust Version](https://img.shields.io/badge/Rust-2024-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-AGPLv3-blue.svg)](https://www.gnu.org/licenses/agpl-3.0.en.html)

**Hilo** is a Rust-based social pairing backend that matches university students based on their interests, traits, and preferences. The system uses email verification with university domains, collects detailed user questionnaires, and employs a sophisticated matching algorithm to pair compatible users.

## Technical Architecture

- **Framework**: Axum web framework with Tokio async runtime
- **Database**: PostgreSQL with SQLx for compile-time checked queries
- **Authentication**: JWT tokens with refresh token support and email verification
- **File Storage**: Local filesystem for user-uploaded images (ID cards and profile photos)
- **Matching System**: Background service with configurable scoring algorithm
- **Email Service**: Trait-based email abstraction supporting multiple providers
- **Tag System**: Hierarchical tag structure with good lookup performance

## User Workflow

### Part I. Authentication & ID Verification

1. **Email Verification**: Users request a verification code sent to their university email
   - Only emails from approved university domains are accepted
   - Rate limiting prevents abuse (configurable interval between requests)
   - 6-digit verification codes expire after a set duration

2. **Account Creation**: Upon successful email verification:
   - User account is created with `unverified` status
   - JWT access and refresh tokens are issued for authentication
   - Users can upload their student ID card for verification, meanwhile the status is `verification_pending`

3. **ID Card Upload**: Users upload a photo of their student ID card
   - Images are validated and stored securely
   - Admin's review changes user status to `verified`/`unverified`

### Part II. Form Submission

1. **Profile Form**: Verified users complete a questionnaire including:
   - Personal information (WeChat ID, gender, self-introduction)
   - Interest tags (familiar and aspirational categories)
   - Personality traits (self-assessment and ideal partner preferences)
   - Expected boundary and recent conversation topics
   - **Optional** profile photo upload

2. **Tag Selection**: Users choose from a hierarchical tag system:
   - Maximum tag limit enforced (configurable)
   - Tags are categorized and have IDF-based scoring for matching

3. **Status Update**: Form completion changes user status to `form_completed`

### Part III. Match Previews & Veto System

Veto means rejection.

1. **Preview Generation**: Background service periodically generates match suggestions:
   - Algorithm considers tag compatibility, trait matching, and expected boundary
   - Matching tags receive higher scores, and complementary tags receive lower scores

2. **User Review**: Users can view a couple of top-score potential matches
   - Displayed info: `familiar_tags`, `aspirational_tags`, `recent_topics`, `email_domain`, `grade`
   - Users can veto unwanted matches based on their info before final pairing
   - Vetoed users are excluded from final matching algorithm

### Part IV. Final Matching & Results

1. **Admin Schedule**: Administrators can schedule final matches or manually trigger the final matching process:
   - A final match will be automatically executed at each scheduled timestamp. Users can use API to get the next timestamp.
   - Only users with `form_completed` status are included, after this their status becomes updated to `matched` (unless unmatched)
   - Vetoes are considered to exclude incompatible pairs
   - Algorithm: **Kuhn Munkres** (maximum weight)

2. **Match Results**: Users receive their final match information and decide if their accept it:
   - Displayed info: `familiar_tags`, `aspirational_tags`, `recent_topics`, `self_intro`, `email_domain`, `grade`, profile photo (if any)
   - A user's status becomes `confirmed` when they accept the match. Once both users accepted the match, `wechat_id` is displayed.
   - Matches that are not rejected or mutually confirmed will be auto-confirmed 24 hours after its creation.
   - A rejection from either side will revert both users' status to `form_completed`. They will participate in the next round of final match.

## API Documentation

<details>
<summary>Public APIs</summary>

### Public APIs

#### Authentication Endpoints

- `POST /api/auth/send-code` - Send verification code to email
  - JSON request body: `email`
  - Rate limited per email address
  - Only accepts university domain emails
  - Returns `202 Accepted`

- `POST /api/auth/verify-code` - Verify email code and get JWT tokens
  - JSON request body: `email`, `code`
  - Creates user account and issues token pair
  - Returns `200 OK` with tokens and expiration time
  - Response:

  ```json
  {
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIyNTM2ZjViMC0wZjZjLTQwMWItOWY5Mi1iZTk1ZWZlNTcxZWQiLCJleHAiOjE3NTcyMjQ2NjcsImlhdCI6MTc1NzIyMTA2N30.cSQ4dJp21ie-JdN1S01RtcMmmbtaAO0BorVuBjOzVro",
    "refresh_token": "d1a9ef00-7030-4eaa-a1f5-ca3b582d2f74",
    "token_type": "Bearer",
    "expires_in": 900
  }
  ```

- `POST /api/auth/refresh` - Refresh JWT token pair
  - JSON request body: `refresh_token`
  - Uses valid refresh token to get new tokens
  - Returns: `200 OK` with new tokens and expiration time, refer to `POST /api/auth/verify-code`

#### Health Check

- `GET /health_check` - Server health status
  - Always returns `200 OK`

</details>

<details>
<summary>Protected APIs</summary>

### Protected APIs

_All protected endpoints require valid JWT Bearer token in Authorization header_

#### Profile Management

- `GET /api/profile` - Get current user profile with their final match partner information if any
  - If the user doesn't have a final match partner, the final result field will be null; wechat_id becomes not null once both sides have accepted the result

  ```json
  {
    "email": "second@mails.tsinghua.edu.cn",
    "status": "matched",
    "grade": "graduate",
    "final_match": {
      "email_domain": "mails.tsinghua.edu.cn",
      "grade": "undergraduate",
      "familiar_tags": ["pc_fps", "spanish"],
      "aspirational_tags": ["volleyball", "creative_games"],
      "recent_topics": "I've been reading Harry Potter",
      "self_intro": "Hello world",
      "photo_url": "/api/images/partner/91f4cf07-b2b4-4c05-a31e-9ed524c936ee.jpg",
      "wechat_id": null
    }
  }
  ```

- `POST /api/upload/profile-photo` - Upload user profile photo
  - Request body: Multipart form with an image `file` field
  - Returns filename for form submission
  - Response: `{"filename": "2536f5b0-0f6c-401b-9f92-be95efe571ed.jpg"}`

#### Form Management

- `POST /api/form` - Submit or update user form
  - Only accessible to verified users; once submitted, it cannot be changed
  - Returns `200 OK` with partial submitted form data (without wechat_id field), see `GET /api/form` response
  - JSON request body:

  ```json
  {
    "wechat_id": "examplewechatid",
    "gender": "female",
    "familiar_tags": ["pc_fps", "spanish"],
    "aspirational_tags": ["volleyball", "creative_games"],
    "recent_topics": "Recently I love Bitcoin",
    "self_traits": ["empathy", "explorer"],
    "ideal_traits": ["empathy", "explorer"],
    "physical_boundary": 3,
    "self_intro": "Hello world",
    "profile_photo_filename": "91f4cf07-b2b4-4c05-a31e-9ed524c936ee.jpg"
  }
  ```

- `GET /api/form` - Retrieve user's submitted form
  - Returns `200 OK` with partial submitted form data (without wechat_id field)
  - Response:

  ```json
  {
    "user_id": "91f4cf07-b2b4-4c05-a31e-9ed524c936ee",
    "gender": "female",
    "familiar_tags": ["pc_fps", "spanish"],
    "aspirational_tags": ["volleyball", "creative_games"],
    "recent_topics": "Recently I love Bitcoin",
    "self_traits": ["empathy", "explorer"],
    "ideal_traits": ["empathy", "explorer"],
    "physical_boundary": 3,
    "self_intro": "Hello world",
    "profile_photo_filename": "91f4cf07-b2b4-4c05-a31e-9ed524c936ee.jpg"
  }
  ```

#### ID Verification

- `POST /api/upload/card` - Upload student ID card for verification
  - Multipart form with ID card image `card` field and `grade` text field
  - Changes user status to verification pending
  - Returns `200 OK` with some user info
  - Response:

  ```json
  {
    "email": "second@mails.tsinghua.edu.cn",
    "status": "verification_pending",
    "grade": "graduate",
    "card_photo_filename": "2536f5b0-0f6c-401b-9f92-be95efe571ed.jpg"
  }
  ```

#### Matching System

- `GET /api/veto/previews` - Get current match previews for user to decide who to give veto
  - Response:

  ```json
  [
    {
      "candidate_id": "3bc5b542-36f2-41d8-8c63-f252f0eb438c",
      "familiar_tags": ["tennis", "martial_arts"],
      "aspirational_tags": ["wild", "pc_fps"],
      "recent_topics": "I'm User 7 and I love meeting new people! I enjoy various activities and am looking forward to connecting with like-minded individuals.",
      "email_domain": "mails.tsinghua.edu.cn",
      "grade": "undergraduate"
    },
    {
      "candidate_id": "47c361f7-d828-4015-892d-bd842bd5b7d7",
      "familiar_tags": ["music_games", "soccer"],
      "aspirational_tags": ["narrative_adventure", "other_sports"],
      "recent_topics": "I'm User 39 and I love meeting new people! I enjoy various activities and am looking forward to connecting with like-minded individuals.",
      "email_domain": "mails.tsinghua.edu.cn",
      "grade": "undergraduate"
    }
  ]
  ```

- `POST /api/veto` - Veto unwanted potential partner
  - JSON request body: `vetoed_id`
  - Response: `{"id": "f217e3c5-b503-4d8d-b37a-251ef63bcf06", "vetoer_id": "91f4cf07-b2b4-4c05-a31e-9ed524c936ee", "vetoed_id": "3bc5b542-36f2-41d8-8c63-f252f0eb438c"}`

- `DELETE /api/veto` - Revoke vetoes
  - JSON request body: `vetoed_id`
  - Response: `{"id": "f217e3c5-b503-4d8d-b37a-251ef63bcf06", "vetoer_id": "91f4cf07-b2b4-4c05-a31e-9ed524c936ee", "vetoed_id": "3bc5b542-36f2-41d8-8c63-f252f0eb438c"}`

- `GET /api/vetoes` - Get casted vetoes
  - Returns `200 OK` with a list of UUIDs of casted vetoes
  - Response: `["3bc5b542-36f2-41d8-8c63-f252f0eb438c", "47c361f7-d828-4015-892d-bd842bd5b7d7"]`

- `GET /api/final-match/time` - Get next scheduled final match time
  - Response: `{"next": null}` or `{"next": "2025-09-17T13:00:59Z"}`

- `POST /api/final-match/accept`, `POST /api/final-match/reject` - Decide on final match
  - Returns `200 OK` with updated profile
  - Response: refer to `GET /api/profile`

- `GET /api/partner-image/{filename}` - Get partner's profile photo
  - Maximum access control, only accessible to matched partners
  - Returns `200 OK` with image

</details>

<details>
<summary>Admin APIs</summary>

### Admin APIs

_Admin endpoints run on separate port (configured via `ADMIN_ADDRESS`)_

#### User Management

- `GET /api/admin/users?...` - Get paginated users overview
  - Query Params: (optional)
    - `page` (default: 1) - Page number
    - `limit` (default: 20, max: 100) - Items per page
    - `status` (default: null, accpetable: `unverified`|`verification_pending`|`verified`|`form_completed`|`matched`|`confirmed`) - Filter a specific status

  ```json
  {
    "data": [
      {
        "id": "91f4cf07-b2b4-4c05-a31e-9ed524c936ee",
        "email": "test@mails.tsinghua.edu.cn",
        "status": "form_completed"
      }
    ],
    "pagination": {
      "page": 1,
      "limit": 20,
      "total": 1,
      "total_pages": 1
    }
  }
  ```

- `GET /api/admin/user/{user_id}` - Get detailed user information

  ```json
  {
    "id": "91f4cf07-b2b4-4c05-a31e-9ed524c936ee",
    "email": "test@mails.tsinghua.edu.cn",
    "status": "form_completed",
    "wechat_id": "examplewechatid",
    "grade": "undergraduate",
    "card_photo_uri": "/api/admin/card/91f4cf07-b2b4-4c05-a31e-9ed524c936ee.jpg",
    "created_at": [2025, 250, 3, 35, 40, 479291000, 0, 0, 0],
    "updated_at": [2025, 250, 3, 56, 32, 487637000, 0, 0, 0],
    "form": {
      "gender": "female",
      "familiar_tags": ["pc_fps", "spanish"],
      "aspirational_tags": ["soccer", "creative_games"],
      "recent_topics": "Recently I love Bitcoin",
      "self_traits": ["empathy", "explorer"],
      "ideal_traits": ["empathy", "explorer"],
      "physical_boundary": 3,
      "self_intro": "Hello world",
      "profile_photo_uri": "/api/admin/photo/91f4cf07-b2b4-4c05-a31e-9ed524c936ee.jpg"
    }
  }
  ```

- `POST /api/admin/verify-user` - Update user verification status
  - JSON request body: `email` or `user_id`, `status`
  - Response:

  ```json
  {
    "user_id": "2536f5b0-0f6c-401b-9f92-be95efe571ed",
    "email": "second@mails.tsinghua.edu.cn",
    "status": "verified",
    "grade": "graduate",
    "card_photo_filename": "2536f5b0-0f6c-401b-9f92-be95efe571ed.jpg"
  }
  ```

- `GET /api/admin/card/{filename}` - Get user student card photo
  - Returns `200 OK` with image

- `GET /api/admin/photo/{filename}` - Get user profile photo
  - Returns `200 OK` with image

#### Analytics & Statistics

- `GET /api/admin/stats` - Get user and system statistics

  ```json
  {
    "total_users": 2,
    "males": 1,
    "females": 1,
    "unmatched_males": 1,
    "unmatched_females": 1
  }
  ```

- `GET /api/admin/tags` - Get tag usage statistics

  ```json
  [
    {
      "id": "sports",
      "name": "运动/户外活动",
      "desc": "各类运动",
      "is_matchable": true,
      "user_count": 0,
      "idf_score": null,
      "children": [
        {
          "id": "volleyball",
          "name": "排球🏐",
          "desc": null,
          "is_matchable": true,
          "user_count": 1,
          "idf_score": 0.6931471805599453,
          "children": null
        }
      ]
    }
  ]
  ```

#### Matching Operations

- `POST /api/admin/update-previews` - Regenerate match previews
  - Response: `{"success": true, "message": "Match previews updated successfully"}`
- `POST /api/admin/trigger-match` - Manually execute final matching immediately (normally won't be used)
  - Response: `{"success": true, "message": "Final matching completed successfully", "matches_created": 0}`
- `GET /api/admin/matches?...` - View all final matches
  - Query Parameters: (optional)
    - `page` (default: 1) - Page number
    - `limit` (default: 20, max: 100) - Items per page
  - Response:

  ```json
  {
    "data": [
      {
        "id": "e5aaeda4-a552-4858-a007-0d2e348987dd",
        "user_a_id": "067c94a2-85a4-4efa-b6e0-d952176f3fbd",
        "user_a_email": "user34@mails.tsinghua.edu.cn",
        "user_b_id": "8afaf1d9-43e3-4614-b7cf-065b50eb1317",
        "user_b_email": "user43@mails.tsinghua.edu.cn",
        "score": 24.737618891240754
      },
      {
        "id": "2e6199c6-d6f6-4a6e-9772-5617324f1d59",
        "user_a_id": "25bff9d6-ae4d-4098-99c9-93d258a1b4fc",
        "user_a_email": "user2@mails.tsinghua.edu.cn",
        "user_b_id": "4c2330c7-4510-4b6f-9ccd-9db7614b15ad",
        "user_b_email": "user41@mails.tsinghua.edu.cn",
        "score": 17.7941106355937
      }
    ],
    "pagination": {
      "page": 1,
      "limit": 2,
      "total": 27,
      "total_pages": 14
    }
  }
  ```

- `GET /api/admin/scheduled-matches` - View scheduled final matches
  - Response:

  ```json
  [
    {
      "id": "6234b8f4-01df-4ec7-b7b8-67dc328c216c",
      "scheduled_time": "2025-09-17T13:00:59Z",
      "status": "Completed",
      "created_at": "2025-09-17T12:41:55.612615Z",
      "executed_at": "2025-09-17T13:01:55.445273Z",
      "matches_created": 0,
      "error_message": null
    },
    {
      "id": "7ec36949-51a2-4352-812e-f9bec48877dc",
      "scheduled_time": "2025-09-18T20:00:00Z",
      "status": "Pending",
      "created_at": "2025-09-17T12:41:55.614084Z",
      "executed_at": null,
      "matches_created": null,
      "error_message": null
    }
  ]
  ```

- `POST /api/admin/scheduled-matches` - Schedule a final match
  - JSON request body: `{"scheduled_times": [{"scheduled_time": "2025-09-17T13:00:59Z"}]}`
  - 201 Created with Response:

  ```json
  [
    {
      "id": "6234b8f4-01df-4ec7-b7b8-67dc328c216c",
      "scheduled_time": "2025-09-17T13:00:59Z",
      "status": "Pending",
      "created_at": "2025-09-17T12:41:55.612615Z",
      "executed_at": null,
      "matches_created": null,
      "error_message": null
    }
  ]
  ```

- `DELETE /api/admin/scheduled-matches/{id}` - Cancel a scheduled final match
  - Returns 200 OK

- `DELETE /api/admin/final-matches/{id}` - Delete a final match and revert users
  - Deletes the final match by ID and reverts both users' status to `form_completed`
  - Useful for correcting matching errors or handling rematch requests
  - Returns 200 OK with `{"success": true, "message": "Final match deleted and users reverted successfully"}`
  - Returns 404 if match not found

</details>

## Quick Start

### Run in container (recommended)

1. **Set up prerequisites**
   - Podman setup in rootless mode, Podman Compose
   - Obtain valid send API key from [Mailgun](https://mailgun.com)

2. **Prepare secrets**: Start somewhere safe and permanent like `~/.config`

```bash
$ umask 077
$ mkdir secrets_hilo && cd secrets_hilo

$ openssl rand -base64 32 > ./jwt_secret.txt

# Then write your mailgun api key and database password into files, for example
$ nvim ./mailgun_api_key.txt
$ nvim ./db_password.txt

$ podman secret create hilo_jwt_secret ./jwt_secret.txt
$ podman secret create hilo_mailgun_api_key ./mailgun_api_key.txt
$ podman secret create hilo_db_password ./db_password.txt
```

3. **Compose and run in background**

```bash
$ curl -o compose.yml https://raw.githubusercontent.com/Mapleshade20/hilo/main/compose.yml
$ podman-compose up -d

# To monitor health, run:
$ curl http://localhost:8090/health-check
$ podman logs hilo_app_1

# To stop, run:
$ podman-compose down
```

4. **Go online**: Configure a reverse proxy on port `8090` and firewall to make it publicly accessible
5. (Optional) Refer to other documentations on how to make it a systemd service

### Run on host machine

<details>
<summary>Details</summary>

1. Configure `.env`
2. Start PostgreSQL with Podman

```bash
$ podman-compose up -f <which.yml> -d db
```

3. Run with cargo

```bash
$ cargo run
```

</details>

## Additional Notes

### Email Service

The email service supports multiple providers:

- **Log Provider** (`EMAIL_PROVIDER="log"`): Logs emails to console (for development)
- **External Provider** (`EMAIL_PROVIDER="external"`): HTTP API with Basic Auth
  - Currently supports Mailgun-style API (username: "api", password: api-key)
  - Configure `SENDER_EMAIL`, `MAIL_API_URL` and `MAIL_API_KEY`(`MAIL_API_KEY_FILE`)

### Deployment Security Considerations

- **Rate Limiting**: The verification code API has a built-in per-email rate limiting, but production deployments should implement IP-based rate limiting and ddos protection for all endpoints
  - TODO: The current implementation of login code cache is not good enough. Should use Redis.
- **System Time**: Ensure accurate system time for JWT token expiration
- **Database Security**: Use strong passwords
- **HTTPS**: Always use HTTPS in production with proper SSL certificates
- **Admin API**: **Do not expose admin endpoints to public network.** Instead, use Cloudflare Access or similar gateways to secure it.

### Environment Variables in Production

Configure environment variables in `compose.yml`. For their meanings refer to `.env`

### User Management Workflow for Admin

1. **ID Card Verification**: Review uploaded ID cards via admin interface
2. **Status Updates**: Change user status from `verification_pending` to `verified`
3. **Match Generation**: Schedule a few final matches in advance
4. **System Monitoring**: Monitor user statistics and system health

## Development

### Development Tools

This repository uses [pre-commit](https://pre-commit.com/) for code quality and [conventional commits](https://www.conventionalcommits.org/en/v1.0.0/):

```bash
# Install pre-commit hooks
$ pre-commit install --hook-type commit-msg
$ pre-commit install --hook-type pre-commit

# Run all checks
$ pre-commit run --all-files
```

SQLx queries should be written to `.sqlx` for offline build

- **Migrations**: `sqlx migrate run` (via sqlx-cli)
- **SQLx offline prepare**: `cargo sqlx prepare -- --all-targets`
- **Start postgres**: `podman-compose -f podman-compose.dev.yml up --detach db`
- **Reset postgres**: `podman-compose -f podman-compose.dev.yml down -v`

### Testing

- Integration tests use `#[sqlx::test]` for automatic test database setup
- Tests use special configurations under `tests/data/`
