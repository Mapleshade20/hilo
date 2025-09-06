# Hilo: Social Pairing Backend for Project Encontrar

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
   - Users can upload their student ID card for verification

3. **ID Card Upload**: Users upload a photo of their student ID card
   - Images are validated and stored securely
   - Admin's review changes user status from `unverified` to `verified`

### Part II. Form Submission

1. **Profile Form**: Verified users complete a questionnaire including:
   - Personal information (WeChat ID, gender, self-introduction)
   - Interest tags (familiar and aspirational categories)
   - Personality traits (self-assessment and ideal partner preferences)
   - Physical boundaries and recent conversation topics
   - **Optional** profile photo upload

2. **Tag Selection**: Users choose from a hierarchical tag system:
   - Maximum tag limit enforced (configurable)
   - Tags are categorized and have IDF-based scoring for matching

3. **Status Update**: Form completion changes user status to `form_completed`

### Part III. Match Previews & Veto System

Veto means rejection.

1. **Preview Generation**: Background service periodically generates match suggestions:
   - Algorithm considers tag compatibility, trait matching, and physical boundaries
   - Matching tags receive higher scores, and complementary tags receive lower scores

2. **User Review**: Users can view a couple of top-score potential matches
   - Displayed info: `familiar_tags`, `aspirational_tags`, `recent_topics`, `email_domain`, `grade`
   - Users can veto unwanted matches based on their info before final pairing
   - Vetoed users are excluded from final matching algorithm

### Part IV. Final Matching & Results

1. **Admin Trigger**: Administrators initiate the final matching process:
   - Only users with `form_completed` status are included
   - Vetoes are considered to exclude incompatible pairs
   - Algorithm: **Greedy**

2. **Match Results**: Users receive their final match information and decide if their accept it:
   - Displayed info: `familiar_tags`, `aspirational_tags`, `self_intro`, `email_domain`, `grade`, profile photo (if any)
   - Once both users accepted the match, `wechat_id` is displayed
   - A rejection from either side will revert both users' status to `form_completed`. Admin can trigger new final matching after a period of time, and only unmatched users will be included in this round.

## API Documentation

<details>
<summary>Public APIs</summary>

### Public APIs

#### Authentication Endpoints

- `POST /api/auth/send-code` - Send verification code to email
  - JSON request body: `email`
  - Rate limited per email address
  - Only accepts university domain emails
  - Returns: `202 Accepted` or error codes

- `POST /api/auth/verify-code` - Verify email code and get JWT tokens
  - JSON request body: `email`, `code`
  - Creates user account and issues token pair
  - Returns: `200 OK` with `AuthResponse` (access_token, refresh_token)

- `POST /api/auth/refresh` - Refresh JWT token pair
  - JSON request body: `refresh_token`
  - Uses valid refresh token to get new tokens
  - Returns: `200 OK` with new `AuthResponse`

#### Health Check

- `GET /health_check` - Server health status
  - Returns: `200 OK` for healthy server

</details>

<details>
<summary>Protected APIs</summary>

### Protected APIs

_All protected endpoints require valid JWT Bearer token in Authorization header_

#### Profile Management

- `GET /api/profile` - Get current user profile information
- `POST /api/upload/profile-photo` - Upload user profile photo
  - Request body: Multipart form with image file
  - Returns: filename for form submission

#### Form Management

- `POST /api/form` - Submit or update user form
  - JSON request body: refer to code
  - Only accessible to verified users
  - Returns: `200 OK` with saved form data

- `GET /api/form` - Retrieve user's submitted form
  - Returns: `200 OK` with form data

#### ID Verification

- `POST /api/upload/card` - Upload student ID card for verification
  - Multipart form with ID card image and `grade` text field
  - Changes user status to verification pending
  - Returns: `200 OK` with user info

#### Matching System

- `GET /api/veto/previews` - Get current match previews for review
  - Returns: `200 OK` with a list of potential partners' UUIDs

- `POST /api/veto` - Veto unwanted potential partner
  - JSON request body: `vetoed_id`

- `DELETE /api/veto` - Revoke vetoes
  - JSON request body: `vetoed_id`

- `GET /api/vetoes` - Get casted vetoes
  - Returns: `200 OK` with a list of UUIDs of casted vetoes

- `POST /api/final-match/accept`, `POST /api/final-match/reject` - Decide on final match

- `GET /api/partner-image/{filename}` - Get partner's profile photo
  - Maximum access control, only accessible to matched partners

</details>

<details>
<summary>Admin APIs</summary>

### Admin APIs

_Admin endpoints run on separate port (configured via `ADMIN_ADDRESS`)_

#### User Management

- `GET /api/admin/users` - Get paginated users overview
- `GET /api/admin/user/{user_id}` - Get detailed user information
- `POST /api/admin/verify-user` - Update user verification status
- `GET /api/admin/card/{filename}` - Get user ID card photos

#### Analytics & Statistics

- `GET /api/admin/stats` - Get user and system statistics
- `GET /api/admin/tags` - Get tag usage statistics

#### Matching Operations

- `POST /api/admin/update-previews` - Regenerate match previews
- `POST /api/admin/trigger-match` - Execute final matching algorithm
- `GET /api/admin/matches` - View all final matches

</details>

## Quick Start

### Run in container (recommended)

TODO!

### Run on host machine

<details>
<summary>Run on host machine</summary>

Configure `.env`

Start PostgreSQL with Podman:

```bash
$ podman-compose up -d db

$ sqlx migrate run

$ cargo run
```

</details>

## Production Deployment

### Email Service

The email service supports multiple providers:

- **Log Provider** (`EMAIL_PROVIDER="log"`): Logs emails to console (development)
- **External Provider** (`EMAIL_PROVIDER="external"`): HTTP API with Basic Auth
  - Currently supports Mailgun-style API (username: "api")
  - Configure `MAIL_API_URL` and `MAIL_API_KEY`

### Security Considerations

- **Rate Limiting**: The verification code API has per-email rate limiting, but production deployments should implement IP-based rate limiting for all endpoints
- **System Time**: Ensure accurate system time for JWT token expiration
- **Database Security**: Use strong passwords and restrict database access
- **File Storage**: Configure secure file permissions for upload directories
- **HTTPS**: Always use HTTPS in production with proper SSL certificates

### Environment Variables for Production

See `.env`

## Admin Operations

### User Management Workflow

1. **ID Card Verification**: Review uploaded ID cards via admin interface
2. **Status Updates**: Change user status from `verification_pending` to `verified`
3. **Match Generation**: Trigger final matching when ready
4. **System Monitoring**: Monitor user statistics and system health

### Matching Process

1. **Preview Generation**: Run `POST /api/admin/update-previews` periodically
2. **User Review Period**: Allow users to review and veto potential matches
3. **Final Matching**: Trigger `POST /api/admin/trigger-match` when ready
4. **Result Monitoring**: Check match quality via `GET /api/admin/matches`

## Development

### Development Tools

This repository uses [pre-commit](https://pre-commit.com/) for code quality and [conventional commits](https://www.conventionalcommits.org/en/v1.0.0/):

```bash
# Install pre-commit hooks
pre-commit install --hook-type commit-msg
pre-commit install --hook-type pre-commit

# Run all checks
pre-commit run --all-files
```

### Testing

- Integration tests use `#[sqlx::test]` for automatic test database setup
- Tests use special configurations under `tests/data/`

```bash
# Run all tests with database
cargo test

# Run specific integration test
cargo test --test authenticate
```
