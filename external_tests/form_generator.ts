// Form generation module for creating user forms with tags
import { FormData, User, TagNode, TagConfig, TestMode } from "./types.ts";
import {
  extractLeafTags,
  generateRandomTags,
  parseConfigFile,
  generateWeChatId,
  colors,
  colorPrint,
} from "./utils.ts";

const HILO_API_URL = "http://127.0.0.1:8090";

// Generate form data based on mode
export function generateFormData(
  user: User,
  mode: TestMode,
  leafTags: string[],
  config?: TagConfig,
): FormData {
  let familiarTags: string[] = [];
  let aspirationalTags: string[] = [];

  if (mode === "random") {
    const tags = generateRandomTags(leafTags, 8);
    familiarTags = tags.familiar;
    aspirationalTags = tags.aspirational;
  } else if (mode === "config" && config) {
    // Extract tags for this user from config
    for (const [tagId, tagUsers] of Object.entries(config)) {
      if (tagUsers.familiar.includes(user.id)) {
        familiarTags.push(tagId);
      }
      if (tagUsers.aspirational.includes(user.id)) {
        aspirationalTags.push(tagId);
      }
    }
  }

  return {
    wechat_id: generateWeChatId(user.id),
    gender: user.gender,
    familiar_tags: familiarTags,
    aspirational_tags: aspirationalTags,
    recent_topics: `I'm User ${user.id} and I love meeting new people! I enjoy various activities and am looking forward to connecting with like-minded individuals.`,
    self_traits: [], // Empty as specified
    ideal_traits: [], // Empty as specified
    physical_boundary: 3, // Set to 3 as specified
    self_intro: `Hello! I'm User ${user.id}. I'm a ${user.gender} student who enjoys ${familiarTags.slice(0, 2).join(" and ")}. Looking forward to making new connections!`,
  };
}

// Submit form for a user
export async function submitForm(
  user: User,
  formData: FormData,
): Promise<void> {
  const response = await fetch(`${HILO_API_URL}/api/form`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: `Bearer ${user.accessToken}`,
    },
    body: JSON.stringify(formData),
  });

  if (!response.ok) {
    throw new Error(
      `Failed to submit form for user ${user.id}: ${response.status} ${response.statusText}`,
    );
  }
}

// Submit forms for all users
export async function submitAllForms(
  users: User[],
  mode: TestMode,
  leafTags: string[],
  configPath?: string,
): Promise<void> {
  console.log(`\n📝 Generating and submitting forms in ${mode} mode...`);

  let config: TagConfig | undefined;
  if (mode === "config" && configPath) {
    try {
      config = parseConfigFile(configPath);
      console.log(
        `📋 Loaded config with ${Object.keys(config).length} tag mappings`,
      );
    } catch (error) {
      colorPrint(
        `⚠️  Failed to load config file, falling back to random mode: ${error instanceof Error ? error.message : String(error)}`,
        colors.yellow,
      );
      mode = "random";
    }
  }

  const formPromises = users.map(async (user) => {
    try {
      const formData = generateFormData(user, mode, leafTags, config);
      await submitForm(user, formData);

      console.log(`✅ User ${user.id} form submitted:`);
      console.log(
        `   Familiar: ${formData.familiar_tags.join(", ") || "none"}`,
      );
      console.log(
        `   Aspirational: ${formData.aspirational_tags.join(", ") || "none"}`,
      );

      return formData;
    } catch (error) {
      colorPrint(
        `❌ Failed to submit form for User ${user.id}: ${error instanceof Error ? error.message : String(error)}`,
        colors.red,
      );
      throw error;
    }
  });

  try {
    await Promise.all(formPromises);
    colorPrint(`\n✅ All forms submitted successfully!`, colors.green);
  } catch (error) {
    colorPrint(
      `❌ Failed to submit all forms: ${error instanceof Error ? error.message : String(error)}`,
      colors.red,
    );
    throw error;
  }
}

// Load tags from tags.json
export async function loadTags(): Promise<string[]> {
  try {
    const tagsContent = await Deno.readTextFile("../tags.json");
    const tags: TagNode[] = JSON.parse(tagsContent);
    const leafTags = extractLeafTags(tags);

    console.log(`📚 Loaded ${leafTags.length} leaf tags from tags.json`);
    console.log(
      `Available tags: ${leafTags.slice(0, 10).join(", ")}${leafTags.length > 10 ? "..." : ""}`,
    );

    return leafTags;
  } catch (error) {
    colorPrint(
      `❌ Failed to load tags.json: ${error instanceof Error ? error.message : String(error)}`,
      colors.red,
    );
    throw error;
  }
}
