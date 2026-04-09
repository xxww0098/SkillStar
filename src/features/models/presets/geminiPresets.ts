export interface GeminiPreset {
  name: string;
  websiteUrl: string;
  apiKeyUrl?: string;
  env: Record<string, string>;
  category: "official" | "cn_official" | "cloud_provider" | "aggregator" | "third_party";
  icon?: string;
  iconColor?: string;
}

export const geminiPresets: GeminiPreset[] = [
  {
    name: "Gemini Official",
    websiteUrl: "https://aistudio.google.com/",
    apiKeyUrl: "https://aistudio.google.com/app/apikey",
    env: {
      GEMINI_API_KEY: "",
    },
    category: "official",
    iconColor: "#3B82F6",
  },
];
