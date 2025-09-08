// TypeScript type definitions for the external test

export interface AuthResponse {
  access_token: string;
  refresh_token: string;
  token_type: string;
  expires_in: number;
}

export interface User {
  id: number;
  email: string;
  accessToken: string;
  gender: "male" | "female";
}

export interface FormData {
  wechat_id: string;
  gender: "male" | "female";
  familiar_tags: string[];
  aspirational_tags: string[];
  recent_topics: string;
  self_traits: string[];
  ideal_traits: string[];
  physical_boundary: number;
  self_intro: string;
  profile_photo_filename?: string;
}

export interface ProfilePreview {
  familiar_tags: string[];
  aspirational_tags: string[];
  recent_topics: string;
  email_domain: string;
  grade: string;
}

export interface TagNode {
  id: string;
  name: string;
  is_matchable: boolean;
  children?: TagNode[];
}

export interface TagConfig {
  [tagId: string]: {
    familiar: number[];
    aspirational: number[];
  };
}

export interface VerificationEmail {
  to: string;
  from: string;
  subject: string;
  text: string;
}

export type TestMode = "random" | "config";
