// Main test orchestrator for the Hilo matching simulation
import { startEmailServer, clearVerificationCodes } from "./email_server.ts";
import { setupUsers } from "./user_setup.ts";
import { loadTags, submitAllForms } from "./form_generator.ts";
import { User, ProfilePreview, TestMode } from "./types.ts";
import {
  printHeader,
  printUserInfo,
  printMatchPreview,
  colors,
  colorPrint,
  sleep,
  setSilentMode,
  log
} from "./utils.ts";

const HILO_API_URL = "http://127.0.0.1:8090";
const ADMIN_API_URL = "http://127.0.0.1:8091";

// Trigger match preview update via admin API
async function updateMatchPreviews(): Promise<void> {
  log(`\n🔄 Triggering match preview update...`);

  const response = await fetch(`${ADMIN_API_URL}/api/admin/update-previews`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to update match previews: ${response.status} ${response.statusText}`);
  }

  const result = await response.json();
  log(`✅ Match previews updated: ${result.message}`);
}

// Get match previews for a user
async function getMatchPreviews(user: User): Promise<ProfilePreview[]> {
  const response = await fetch(`${HILO_API_URL}/api/veto/previews`, {
    method: "GET",
    headers: {
      "Authorization": `Bearer ${user.accessToken}`,
    },
  });

  if (!response.ok) {
    throw new Error(`Failed to get match previews for user ${user.id}: ${response.status} ${response.statusText}`);
  }

  return await response.json();
}

// Display match results for all users
async function displayMatchResults(users: User[]): Promise<void> {
  printHeader("MATCH RESULTS");

  for (const user of users) {
    try {
      printUserInfo(user.id, user.email, user.gender);

      const previews = await getMatchPreviews(user);

      if (previews.length === 0) {
        colorPrint(`  No matches found 😔`, colors.dim);
      } else {
        colorPrint(`  Found ${previews.length} match${previews.length > 1 ? "es" : ""} 🎉`, colors.green);

        previews.forEach((preview, index) => {
          printMatchPreview(preview, index);
        });
      }

      log(""); // Empty line between users
    } catch (error) {
      colorPrint(`  ❌ Error fetching matches: ${error instanceof Error ? error.message : String(error)}`, colors.red);
    }
  }
}

// Parse command line arguments
function parseArgs(): { mode: TestMode; userCount: number; maleCount?: number; configPath?: string; fullMode: boolean; silent: boolean } {
  const args = Deno.args;

  let mode: TestMode = "random";
  let userCount = 6;
  let maleCount: number | undefined;
  let configPath: string | undefined;
  let fullMode = false;
  let silent = false;

  for (let i = 0; i < args.length; i++) {
    switch (args[i]) {
      case "--mode":
        if (args[i + 1] === "config" || args[i + 1] === "random") {
          mode = args[i + 1] as TestMode;
          i++;
        }
        break;
      case "--users":
        const count = parseInt(args[i + 1]);
        if (!isNaN(count) && count > 0) {
          userCount = count;
          i++;
        }
        break;
      case "--males":
        const males = parseInt(args[i + 1]);
        if (!isNaN(males) && males >= 0) {
          maleCount = males;
          i++;
        }
        break;
      case "--config":
        configPath = args[i + 1];
        i++;
        break;
      case "--full":
        fullMode = true;
        break;
      case "--silent":
        silent = true;
        break;
    }
  }

  return { mode, userCount, maleCount, configPath, fullMode, silent };
}

// Main function
async function main(): Promise<void> {
  const { mode, userCount, maleCount, configPath, fullMode, silent } = parseArgs();

  // Set silent mode globally
  setSilentMode(silent);

  printHeader("HILO MATCHING SIMULATION");
  log(`📊 Mode: ${mode}`);
  log(`👥 Users: ${userCount}`);
  if (maleCount !== undefined) {
    log(`👨 Males: ${maleCount}, 👩 Females: ${userCount - maleCount}`);
  }
  if (configPath) {
    log(`📋 Config: ${configPath}`);
  }
  if (fullMode) {
    log(`🎲 Full randomization: enabled`);
  }
  log("");

  try {
    // Step 1: Start email server
    colorPrint("🚀 Starting email server...", colors.cyan);
    const emailServer = await startEmailServer();
    await sleep(1000); // Give server time to start

    // Step 2: Load tags
    colorPrint("📚 Loading tags...", colors.cyan);
    const leafTags = await loadTags();

    // Step 3: Setup users
    clearVerificationCodes();
    const users = await setupUsers(userCount, maleCount);

    // Step 4: Submit forms
    await submitAllForms(users, mode, leafTags, configPath, fullMode);

    // Step 5: Update match previews
    await updateMatchPreviews();

    // Give matching algorithm time to complete
    log("⏳ Waiting for matching algorithm to complete...");
    await sleep(2000);

    // Step 6: Display results
    await displayMatchResults(users);

    printHeader("SIMULATION COMPLETE");
    colorPrint("🎉 All operations completed successfully!", colors.green);

    // Clean shutdown
    emailServer.shutdown();

  } catch (error) {
    colorPrint(`❌ Simulation failed: ${error instanceof Error ? error.message : String(error)}`, colors.red);
    console.error(error);
    Deno.exit(1);
  }
}

// Run if this file is the main module
if (import.meta.main) {
  await main();
}
