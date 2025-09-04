# Hilo: the backend of Project Encontrar

This is the backend of a social pairing project. It authenticates male and female users with their school emails, collects their completed form (a questionnaire about expectations and personal info), finds a best match based on their interests and expectations, and provides query, submission and admin-trigger APIs.

## Design

### Part I. Auth & Verify Card

### Part II. Submit Form

### Part III. Preview & Veto

### Part IV. View Final Result

TODO:

- once a final match is triggered, clear all vetoes and previews in database.
- add a accept/reject final match result api for `matched` status users; if the user reject, revert to `form_completed` status for them and their partner, clear the previews. (so they and their partner, along with other unmatched users, will wait for the next round)
- change the get profile api to return the partner's profile (only the fields designated in `FinalPartnerProfile`) if self's status is `matched` or `confirmed`.

## Notes for new developers

This repository uses [pre-commit](https://pre-commit.com/) to check correct formatting, prevent undesired issues and enforce [conventional commits](https://www.conventionalcommits.org/en/v1.0.0/). Make sure you run the following check before PR.

```
$ pre-commit install --hook-type commit-msg
$ pre-commit install --hook-type pre-commit
$ pre-commit run --all-files
```

The external email service provider only supports HTTP Basic Authentication ("username:apikey") now, and the username defaults to "api" because it's how Mailgun works. In the future more available options will be added.

## Notes for admin

- The verification code sending API implements a per-email rate limit on server side. But in production environment it's necessary to configure cloud service with a stricter IP-based rate limit for **all** APIs.
- Check system time before deploy.
