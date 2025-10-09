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

interface Trait {
  id: string;
  name: string;
}

const HILO_API_URL = "http://127.0.0.1:8090";

// Generate form data based on mode
export function generateFormData(
  user: User,
  mode: TestMode,
  leafTags: string[],
  config?: TagConfig,
  fullMode: boolean = false,
  traits: string[] = [],
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

  // Generate random traits and physical_boundary if fullMode is enabled
  let selfTraits: string[] = [];
  let idealTraits: string[] = [];
  let physicalBoundary = 3;

  if (fullMode && traits.length > 0) {
    // Randomize physical_boundary between 1 and 4
    physicalBoundary = Math.floor(Math.random() * 4) + 1;

    // Randomly select 3 traits for self_traits
    const shuffledTraits = [...traits].sort(() => Math.random() - 0.5);
    selfTraits = shuffledTraits.slice(0, 3);

    // Randomly select 3 different traits for ideal_traits
    const remainingTraits = shuffledTraits.slice(3);
    idealTraits = remainingTraits.slice(0, 3);
  }

  return {
    wechat_id: generateWeChatId(user.id),
    gender: user.gender,
    familiar_tags: familiarTags,
    aspirational_tags: aspirationalTags,
    recent_topics: `I'm User ${user.id} and I love meeting new people! I enjoy various activities and am looking forward to connecting with like-minded individuals.`,
    self_traits: selfTraits,
    ideal_traits: idealTraits,
    physical_boundary: physicalBoundary,
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
  fullMode: boolean = false,
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
        `!  Failed to load config file, falling back to random mode: ${error instanceof Error ? error.message : String(error)}`,
        colors.yellow,
      );
      mode = "random";
    }
  }

  // Load traits if fullMode is enabled
  let traits: string[] = [];
  if (fullMode) {
    try {
      traits = await loadTraits();
    } catch (error) {
      colorPrint(
        `⚠️  Failed to load traits, continuing without traits: ${error instanceof Error ? error.message : String(error)}`,
        colors.yellow,
      );
    }
  }

  const formPromises = users.map(async (user) => {
    try {
      const formData = generateFormData(user, mode, leafTags, config, fullMode, traits);
      await submitForm(user, formData);

      console.log(`✅ User ${user.id} form submitted:`);
      console.log(
        `   Familiar: ${formData.familiar_tags.join(", ") || "none"}`,
      );
      console.log(
        `   Aspirational: ${formData.aspirational_tags.join(", ") || "none"}`,
      );
      if (fullMode) {
        console.log(
          `   Self traits: ${formData.self_traits.join(", ") || "none"}`,
        );
        console.log(
          `   Ideal traits: ${formData.ideal_traits.join(", ") || "none"}`,
        );
        console.log(`   Physical boundary: ${formData.physical_boundary}`);
      }

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

// Load traits from traits.json
export async function loadTraits(): Promise<string[]> {
  try {
    const traitsContent = await Deno.readTextFile("../traits.json");
    const traits: Trait[] = JSON.parse(traitsContent);
    const traitIds = traits.map(trait => trait.id);

    console.log(`🎭 Loaded ${traitIds.length} traits from traits.json`);

    return traitIds;
  } catch (error) {
    colorPrint(
      `❌ Failed to load traits.json: ${error instanceof Error ? error.message : String(error)}`,
      colors.red,
    );
    throw error;
  }
}
