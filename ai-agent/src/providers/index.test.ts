import { describe, expect, it } from "vitest";
import { providerFromEnv } from "./index.js";

describe("providerFromEnv", () => {
  it("selects the mock provider", () => {
    expect(providerFromEnv({ LLM_PROVIDER: "mock" }).name).toBe("mock");
  });

  it("selects anthropic when a key is present", () => {
    const p = providerFromEnv({ LLM_PROVIDER: "anthropic", ANTHROPIC_API_KEY: "sk-test" });
    expect(p.name).toBe("anthropic");
  });

  it("selects openai when a key is present", () => {
    const p = providerFromEnv({ LLM_PROVIDER: "openai", OPENAI_API_KEY: "sk-test" });
    expect(p.name).toBe("openai");
  });

  it("selects ollama (no key needed)", () => {
    expect(providerFromEnv({ LLM_PROVIDER: "ollama" }).name).toBe("ollama");
  });

  it("selects openai against an OpenAI-compatible base URL (e.g. Nosana inference / vLLM)", () => {
    const p = providerFromEnv({
      LLM_PROVIDER: "openai",
      OPENAI_API_KEY: "nosana-key",
      OPENAI_BASE_URL: "https://job.example.node.k8s.prd.nos.ci/v1",
      OPENAI_MODEL: "llama-3.1-8b-instruct",
    });
    expect(p.name).toBe("openai");
  });

  it("throws when the required API key is missing", () => {
    expect(() => providerFromEnv({ LLM_PROVIDER: "anthropic" })).toThrow();
    expect(() => providerFromEnv({ LLM_PROVIDER: "openai" })).toThrow();
  });

  it("throws on an unknown provider", () => {
    expect(() => providerFromEnv({ LLM_PROVIDER: "gemini" })).toThrow();
  });

  it("defaults to anthropic when LLM_PROVIDER is unset", () => {
    expect(() => providerFromEnv({})).toThrow(); // anthropic selected but no key
    expect(providerFromEnv({ ANTHROPIC_API_KEY: "sk-test" }).name).toBe("anthropic");
  });
});
