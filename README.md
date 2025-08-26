# Hilo: the backend of Project Encontrar

## Notes for developers

- Run `pre-commit install` after cloning the repo
- The verification code sending API implements a per-email rate limit on server side. But in production environment it's necessary to configure cloud service with a stricter IP-based rate limit for all APIs.
- The external email service provider only supports HTTP Basic Authentication ("username:apikey") now, and the username is set to be "api" because it's how Mailgun works. In the future we will add more available options to this.
