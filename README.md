# Hilo: the backend of Project Encontrar

This is the backend of a social pairing project. It authenticates male and female users with their school emails, collects their completed form (a questionnaire about expectations and personal info), finds a best match based on their interests and expectations, and provides query, submission and admin-trigger APIs.

## Design

### Part I. Auth

New user visit homepage -> user try to log in with email

=> (IF valid school email domain in env var list) send verification code

=> (IF correct code entered within expiration time) add new user with status `unverified`, give access token & refresh token

-> fetch `/api/profile`

=> (IF `unverified`) frontend auto redirect to student card upload page (ELSE) (END) -> submit image and their grade, status updated to `verification_pending`. _only `unverified` status user have access to upload_

=> (WAIT UNTIL) admin check card with email => (IF valid) admin update status to `verified` (ELSE) admin update status to `unverified`

Registered but logged out user visit homepage -> user try to log in with email => ... => (IF correct code entered within expiration time) give token but do not update status -> fetch `/api/profile` => ...

Upload card api details:

1. Use `multipart/form-data` to transfer image and grade(text) via POST /api/upload/card. Acceptable grades are defined in `.env`.
2. Axum's extractor accepts payload no more than 2MB by default.
3. Store the image in local file system. (`UPLOAD_DIR` defined in `.env`) `UPLOAD_DIR/card_photos/3b6f.....` (after the underscore is user's uuid). The file path is stored in database user table. Newly uploaded image from one user should overwrite the previous image if it exists.

### Part II. Submit Form

#### 1. Tag Configuration (`tags.json`)

- Create a `tags.json` file in the project's configuration directory.
- **Structure**: It will be an array of `TagNode` objects. Each node have:
  - `id`: A unique string identifier (e.g., `games_console_fps`). This is the value stored in the database and sent via the API.
  - `name`: A human-readable string for the UI. _It is only used for frontend display._
  - `desc`: (optional) A short description. _It is only used for frontend display._
  - `children`: (optional) An array of child `TagNode` objects, defining the tree structure.
  - `is_matchable`: A boolean. If `false`, this tag and any of its ancestors cannot be used for scoring common matches.

#### 2. Backend Tag Processing (`TagSystem`)

- **Goal:** To load `tags.json` once at application startup into a highly efficient in-memory structure for fast lookups during scoring.
- **Action:** Implement a `TagSystem` struct.
  - On initialization (e.g., `TagSystem::from_json(...)`), it should parse the `tags.json` file.
  - It will populate two `HashMap`s for O(1) lookups:
    1. `parent_map: HashMap<String, String>`: Maps a child tag ID to its direct parent tag ID. This makes finding all ancestors of a tag extremely fast.
    2. `matchable_map: HashMap<String, bool>`: Maps a tag ID to its `is_matchable` status.
  - This `TagSystem` instance will be part of the shared application state.

#### 3. Database Schema (`forms` table)

- Create a database migration for a table named `forms`. `familiar_tags` and `aspirational_tags` store the `id`s.
- These columns will only store the **leaf** node `id`s selected by the user. The hierarchical logic is handled entirely in the application layer, not in the database.

#### 4. API Endpoints (`/api/form`)

- **Action:** Implement two protected API endpoints that can only be accessed by `verified` and `form_completed` users.
  - **`POST /api/form`**:
    - Accepts a JSON body matching the structure described at the top.
    - Performs validation (total tags no more than env var TOTAL_TAGS, valid enum values). NOTE: profile_photo_path is optional, and if it exists it's been uploaded by user calling api before submitting form, and the path will be included in the user's request, so backend needs to verify corresponding file already exists and the uuid in photo path belongs to this user.
    - Uses an `INSERT ... ON CONFLICT (user_id) DO UPDATE` query to save or update the user's form data in the `forms` table.
  - **`GET /api/form`**:
    - Fetches and returns the currently logged-in user's form data from the `forms` table.
    - Returns a 404 error if the user has not submitted a form yet.

## Notes for new developers

- Run `pre-commit install` after cloning the repo
- The external email service provider only supports HTTP Basic Authentication ("username:apikey") now, and the username defaults to "api" because it's how Mailgun works. In the future more available options will be added.

## Notes for admin

- The verification code sending API implements a per-email rate limit on server side. But in production environment it's necessary to configure cloud service with a stricter IP-based rate limit for **all** APIs.
