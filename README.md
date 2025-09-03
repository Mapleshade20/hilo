# Hilo: the backend of Project Encontrar

This is the backend of a social pairing project. It authenticates male and female users with their school emails, collects their completed form (a questionnaire about expectations and personal info), finds a best match based on their interests and expectations, and provides query, submission and admin-trigger APIs.

## Design

### Part I. Auth & Verify Card

### Part II. Submit Form

### Part III. Preview & Veto

### Part IV. View Final Result

## Notes for new developers

- Run `pre-commit install` after cloning the repo
- The external email service provider only supports HTTP Basic Authentication ("username:apikey") now, and the username defaults to "api" because it's how Mailgun works. In the future more available options will be added.

## Notes for admin

- The verification code sending API implements a per-email rate limit on server side. But in production environment it's necessary to configure cloud service with a stricter IP-based rate limit for **all** APIs.
