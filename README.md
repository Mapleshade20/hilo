# Hilo: the backend of Project Encontrar

This is the backend of a social matching project. It authenticates male and female users with their school emails, collects their completed form (a questionnaire about expectations and personal info), finds a best match based on their interests and expectations, and provides query, submission and admin-trigger APIs.

## Design

Part I. Auth

New user visit homepage -> user try to log in with email
=> (IF valid school email domain in env var list) send verification code
=> (IF correct code entered within expiration time) add new user with status `unverified`, give access token & refresh token
-> fetch `/api/profile`
=> (IF `unverified`) frontend auto redirect to student card upload page (ELSE) (END) -> upload image, update status to `verification_pending`. _note: only `unverified` status user have access to upload_
=> (WAIT UNTIL) admin check card with email => (IF valid) admin update status to `verified` (ELSE) admin update status to `unverified`

Registered but logged out user visit homepage -> user try to log in with email
=> ... same ... => (IF correct code entered within expiration time) give token but do not update status
-> fetch `/api/profile`
=> ... same ...

Upload card api details:

1. Use `multipart/form-data` to transfer image via POST /api/upload-card.
2. Axum's extractor accepts no more than 2MB payload by default, which is good for this project, so no extra tweaks are needed.
3. Must do verification: first check if content-type is image, then use `image` crate's `image::guess_format` function to check if format is PNG/JPG/WEBP.
4. Store the image in local file system. (`UPLOAD_DIR` defined in `.env`) `UPLOAD_DIR/card_photos/user_3b6f7.....` (after the underscore is user's uuid). Make sure to create_dir_all before writing. The file path should be stored in database user table. Newly uploaded image from one user should overwrite the previous image if it exists.

Part II. Submit Form

todo

## Notes for new developers

- Run `pre-commit install` after cloning the repo
- The external email service provider only supports HTTP Basic Authentication ("username:apikey") now, and the username defaults to "api" because it's how Mailgun works. In the future we will add more available options.

## Notes for admin

- The verification code sending API implements a per-email rate limit on server side. But in production environment it's necessary to configure cloud service with a stricter IP-based rate limit for all APIs.
