// User setup module for authentication, card upload, and verification
import { AuthResponse, User } from "./types.ts";
import { waitForVerificationCode } from "./email_server.ts";
import { createTestWebpImage, generateTestEmail, colors, colorPrint, sleep } from "./utils.ts";

const HILO_API_URL = "http://127.0.0.1:8090";
const ADMIN_API_URL = "http://127.0.0.1:8091";

// Send verification code to email
async function sendVerificationCode(email: string): Promise<void> {
  const response = await fetch(`${HILO_API_URL}/api/auth/send-code`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ email }),
  });

  if (!response.ok) {
    throw new Error(`Failed to send verification code: ${response.status} ${response.statusText}`);
  }
}

// Verify code and get access token
async function verifyCode(email: string, code: string): Promise<AuthResponse> {
  const response = await fetch(`${HILO_API_URL}/api/auth/verify-code`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ email, code }),
  });

  if (!response.ok) {
    throw new Error(`Failed to verify code: ${response.status} ${response.statusText}`);
  }

  return await response.json();
}

// Upload card with image and grade
async function uploadCard(accessToken: string, grade: string): Promise<void> {
  const imageData = createTestWebpImage();

  const formData = new FormData();
  formData.append("card", new Blob([imageData], { type: "image/webp" }), "card.webp");
  formData.append("grade", grade);

  const response = await fetch(`${HILO_API_URL}/api/upload/card`, {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${accessToken}`,
    },
    body: formData,
  });

  if (!response.ok) {
    throw new Error(`Failed to upload card: ${response.status} ${response.statusText}`);
  }
}

// Admin verification of user
async function verifyUserAsAdmin(email: string): Promise<void> {
  const response = await fetch(`${ADMIN_API_URL}/api/admin/verify-user`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      email,
      status: "verified"
    }),
  });

  if (!response.ok) {
    throw new Error(`Failed to verify user: ${response.status} ${response.statusText}`);
  }
}

// Complete user setup process
export async function setupUser(userId: number): Promise<User> {
  const email = generateTestEmail(userId);
  const gender = userId % 2 === 1 ? "male" : "female";
  const grade = Math.random() > 0.5 ? "undergraduate" : "graduate";

  colorPrint(`Setting up User ${userId} (${gender}): ${email}`, colors.cyan);

  try {
    // Step 1: Send verification code
    console.log(`  📧 Sending verification code...`);
    await sendVerificationCode(email);

    // Step 2: Wait for verification code from email server
    console.log(`  ⏳ Waiting for verification code...`);
    const code = await waitForVerificationCode(email, 10000);
    console.log(`  🔑 Received verification code: ${code}`);

    // Step 3: Verify code and get access token
    console.log(`  🔐 Verifying code...`);
    const authResponse = await verifyCode(email, code);
    console.log(`  ✅ Got access token`);

    // Step 4: Upload card
    console.log(`  📷 Uploading card (${grade})...`);
    await uploadCard(authResponse.access_token, grade);
    console.log(`  ✅ Card uploaded, status: verification_pending`);

    // Step 5: Admin verification
    console.log(`  👨‍💼 Admin verifying user...`);
    await sleep(500); // Small delay to ensure database consistency
    await verifyUserAsAdmin(email);
    console.log(`  ✅ User verified by admin`);

    colorPrint(`✅ User ${userId} setup complete!`, colors.green);

    return {
      id: userId,
      email,
      accessToken: authResponse.access_token,
      gender,
    };
  } catch (error) {
    colorPrint(`❌ Failed to setup User ${userId}: ${error instanceof Error ? error.message : String(error)}`, colors.red);
    throw error;
  }
}

// Setup multiple users concurrently
export async function setupUsers(userCount: number): Promise<User[]> {
  console.log(`\n🚀 Setting up ${userCount} users...`);

  const setupPromises = Array.from({ length: userCount }, (_, i) =>
    setupUser(i + 1)
  );

  try {
    const users = await Promise.all(setupPromises);
    colorPrint(`\n✅ All ${userCount} users setup complete!`, colors.green);
    return users;
  } catch (error) {
    colorPrint(`❌ Failed to setup users: ${error instanceof Error ? error.message : String(error)}`, colors.red);
    throw error;
  }
}
