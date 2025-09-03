// Utility functions for the external test
import { TagNode, TagConfig } from "./types.ts";

// ANSI color codes for terminal output
export const colors = {
  reset: "\x1b[0m",
  bright: "\x1b[1m",
  dim: "\x1b[2m",
  red: "\x1b[31m",
  green: "\x1b[32m",
  yellow: "\x1b[33m",
  blue: "\x1b[34m",
  magenta: "\x1b[35m",
  cyan: "\x1b[36m",
  white: "\x1b[37m",
  bg_red: "\x1b[41m",
  bg_green: "\x1b[42m",
  bg_blue: "\x1b[44m",
};

// Color printing functions
export function colorPrint(text: string, color: string): void {
  console.log(`${color}${text}${colors.reset}`);
}

export function printHeader(text: string): void {
  console.log(`\n${colors.bright}${colors.cyan}${"=".repeat(60)}${colors.reset}`);
  console.log(`${colors.bright}${colors.cyan}${text.padStart((60 + text.length) / 2)}${colors.reset}`);
  console.log(`${colors.bright}${colors.cyan}${"=".repeat(60)}${colors.reset}\n`);
}

export function printUserInfo(userId: number, email: string, gender: "male" | "female"): void {
  const genderColor = gender === "male" ? colors.blue : colors.magenta;
  console.log(`${colors.bright}User ${userId}:${colors.reset} ${genderColor}${gender}${colors.reset} - ${colors.dim}${email}${colors.reset}`);
}

export function printTagInfo(title: string, tags: string[]): void {
  if (tags.length > 0) {
    console.log(`  ${colors.yellow}${title}:${colors.reset} ${tags.join(", ")}`);
  }
}

export function printMatchPreview(preview: any, index: number): void {
  console.log(`\n${colors.green}  Match Preview ${index + 1}:${colors.reset}`);
  console.log(`    ${colors.cyan}Domain:${colors.reset} ${preview.email_domain}`);
  console.log(`    ${colors.cyan}Grade:${colors.reset} ${preview.grade}`);
  printTagInfo("Familiar Tags", preview.familiar_tags);
  printTagInfo("Aspirational Tags", preview.aspirational_tags);
  console.log(`    ${colors.cyan}Recent Topics:${colors.reset} ${colors.dim}${preview.recent_topics}${colors.reset}`);
}

// Create a 1x1 WEBP image
export function createTestWebpImage(): Uint8Array {
  // Simple 1x1 WEBP image data
  const webpData = new Uint8Array([
    0x52, 0x49, 0x46, 0x46, // "RIFF"
    0x1A, 0x00, 0x00, 0x00, // File size (26 bytes)
    0x57, 0x45, 0x42, 0x50, // "WEBP"
    0x56, 0x50, 0x38, 0x20, // "VP8 "
    0x0E, 0x00, 0x00, 0x00, // VP8 chunk size
    0x30, 0x01, 0x00, 0x9D, 0x01, 0x2A, // VP8 header
    0x01, 0x00, 0x01, 0x00, // Width and height (1x1)
    0x00, 0xFE, 0xFB, 0xFD // VP8 data
  ]);
  return webpData;
}

// Parse leaf tags from tags.json structure
export function extractLeafTags(tags: TagNode[]): string[] {
  const leafTags: string[] = [];
  
  function traverse(nodes: TagNode[]) {
    for (const node of nodes) {
      if (node.is_matchable && (!node.children || node.children.length === 0)) {
        leafTags.push(node.id);
      } else if (node.children) {
        traverse(node.children);
      }
    }
  }
  
  traverse(tags);
  return leafTags;
}

// Generate random tags ensuring no overlap and max limit
export function generateRandomTags(availableTags: string[], maxTotal = 8): { familiar: string[], aspirational: string[] } {
  const shuffled = [...availableTags].sort(() => Math.random() - 0.5);
  const totalTags = Math.min(Math.floor(Math.random() * maxTotal) + 1, availableTags.length);
  const selectedTags = shuffled.slice(0, totalTags);
  
  // Split randomly between familiar and aspirational
  const familiarCount = Math.floor(Math.random() * selectedTags.length);
  const familiar = selectedTags.slice(0, familiarCount);
  const aspirational = selectedTags.slice(familiarCount);
  
  return { familiar, aspirational };
}

// Parse config.txt file
export function parseConfigFile(configPath: string): TagConfig {
  try {
    const content = Deno.readTextFileSync(configPath);
    const config: TagConfig = {};
    
    const lines = content.split('\n').filter(line => line.trim() && !line.startsWith('#'));
    
    for (const line of lines) {
      const [tagPart, aspirationalPart] = line.split('|').map(s => s.trim());
      const [tagId, familiarUsersStr] = tagPart.split(':').map(s => s.trim());
      
      const familiarUsers = familiarUsersStr ? 
        familiarUsersStr.split(',').map(s => parseInt(s.trim())).filter(n => !isNaN(n)) : [];
      
      const aspirationalUsers = aspirationalPart ? 
        aspirationalPart.split(',').map(s => parseInt(s.trim())).filter(n => !isNaN(n)) : [];
      
      config[tagId] = {
        familiar: familiarUsers,
        aspirational: aspirationalUsers
      };
    }
    
    return config;
  } catch (error) {
    console.error(`Error reading config file: ${error instanceof Error ? error.message : String(error)}`);
    return {};
  }
}

// Sleep utility
export function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

// Generate test email with allowed domain
export function generateTestEmail(userId: number): string {
  return `user${userId}@mails.tsinghua.edu.cn`;
}

// Generate WeChat ID
export function generateWeChatId(userId: number): string {
  return `wechat_user_${userId}`;
}