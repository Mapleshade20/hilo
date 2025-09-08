# Hilo External Matching Test

This directory contains a comprehensive external test for the Hilo social pairing system. The test simulates a realistic scenario with multiple users going through the complete flow from registration to viewing match results.

## Files

- `main.ts` - Main test orchestrator
- `email_server.ts` - Mock email server for capturing verification codes
- `user_setup.ts` - User registration, verification, and card upload
- `form_generator.ts` - Form submission with tag assignments
- `utils.ts` - Utility functions and color output
- `types.ts` - TypeScript type definitions
- `config.txt` - Sample configuration for tag assignments

## Usage

### Basic Usage (Random Mode)

```bash
deno run --allow-all main.ts
```

### Specify User Count

```bash
deno run --allow-all main.ts --users 10
```

### Use Configuration Mode

```bash
deno run --allow-all main.ts --mode config --config config.txt
```

### Full Example

```bash
deno run --allow-all main.ts --mode config --users 12 --config config.txt
```

## Test Flow

1. **Email Server Setup** - Starts mock email server on port 8092
2. **User Registration** - Creates users with authentication flow
3. **Card Upload** - Uploads student cards and sets grade
4. **Admin Verification** - Verifies users via admin API
5. **Form Submission** - Submits forms with tag preferences
6. **Match Generation** - Triggers match preview update
7. **Results Display** - Shows match results with color formatting

## Configuration File Format

The `config.txt` file allows you to specify which users get which tags:

```
tag_id: familiar_users | aspirational_users

volleyball: 1, 2, 3 | 5, 9
basketball: 4, 5 | 1, 2
```

- Users are numbered starting from 1
- Odd numbers = male users, even numbers = female users
- Tags must be valid leaf tags from `tags.json`

## Requirements

- Deno runtime
- Hilo server running on port 8090
- Admin server running on port 8091
- PostgreSQL database set up and migrated

## Output

The test provides colored console output showing:

- User setup progress
- Form submission details
- Match results with tag information
- Error messages if any step fails
