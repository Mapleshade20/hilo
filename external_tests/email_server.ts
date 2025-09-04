// Email server mock that receives verification emails and extracts codes
import { VerificationEmail } from "./types.ts";

const PORT = 8092;
const verificationCodes: Map<string, string> = new Map();

// Extract verification code from HTML email body
function extractVerificationCode(htmlBody: string): string | null {
  // Look for the hidden preheader text that contains the verification code
  const preheaderMatch = htmlBody.match(/Your verification code is: (\d{6})/);
  return preheaderMatch ? preheaderMatch[1] : null;
}

// Handle incoming email requests
async function handleEmailRequest(request: Request): Promise<Response> {
  if (request.method !== "POST") {
    return new Response("Method not allowed", { status: 405 });
  }

  try {
    const emailData: VerificationEmail = await request.json();
    console.log(`📧 Received email for: ${emailData.to}`);

    if (emailData.content && emailData.content.length > 0) {
      const htmlContent = emailData.content[0].value;
      const code = extractVerificationCode(htmlContent);

      if (code) {
        verificationCodes.set(emailData.to, code);
        console.log(`🔑 Extracted verification code for ${emailData.to}: ${code}`);
      } else {
        console.log(`❌ Could not extract verification code from email to ${emailData.to}`);
      }
    }

    return new Response("OK", { status: 200 });
  } catch (error) {
    console.error("Error processing email:", error);
    return new Response("Internal Server Error", { status: 500 });
  }
}

// Start the email server
export function startEmailServer(): Promise<Deno.HttpServer> {
  const handler = (request: Request): Promise<Response> => {
    const url = new URL(request.url);

    if (url.pathname === "/" && request.method === "POST") {
      return handleEmailRequest(request);
    }

    return Promise.resolve(new Response("Not Found", { status: 404 }));
  };

  console.log(`🚀 Starting email server on port ${PORT}...`);
  const server = Deno.serve({ port: PORT }, handler);

  return Promise.resolve(server);
}

// Get verification code for email
export function getVerificationCode(email: string): string | undefined {
  return verificationCodes.get(email);
}

// Clear all stored verification codes
export function clearVerificationCodes(): void {
  verificationCodes.clear();
}

// Wait for verification code to arrive
export async function waitForVerificationCode(email: string, timeoutMs = 5000): Promise<string> {
  const startTime = Date.now();

  while (Date.now() - startTime < timeoutMs) {
    const code = getVerificationCode(email);
    if (code) {
      return code;
    }
    await new Promise(resolve => setTimeout(resolve, 100));
  }

  throw new Error(`Timeout waiting for verification code for ${email}`);
}
