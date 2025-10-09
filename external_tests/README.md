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

### Full Random Mode (include traits and boundaries)

```bash
deno run --allow-all main.ts --users 6 --full
```

### Specify User Count

```bash
deno run --allow-all main.ts --users 10
```

### Specify Male Count in Random Mode

```bash
deno run --allow-all main.ts --users 10 --males 7
```

This creates 7 male users and 3 female users. Without `--males`, the default behavior is odd user IDs are male and even user IDs are female.

### Use Configuration Mode

```bash
deno run --allow-all main.ts --mode config --config config.txt
```

### Full Example

```bash
deno run --allow-all main.ts --mode config --users 12 --config config.txt
```

### Advanced Examples

```bash
# 10 users with 8 males, 2 females, full randomization
deno run --allow-all main.ts --users 10 --males 8 --full

# 20 users with 12 males, 8 females
deno run --allow-all main.ts --users 20 --males 12
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
