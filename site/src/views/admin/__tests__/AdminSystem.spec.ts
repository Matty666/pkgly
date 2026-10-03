// ABOUTME: Verifies administrator single sign-on settings load and save correctly.
// ABOUTME: Covers edited OAuth2 payloads, including Casbin access rules.
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { defineComponent, h } from "vue";

vi.mock("@vue/devtools-kit", () => ({}));

const httpGet = vi.fn();
const httpPost = vi.fn();
const httpPut = vi.fn();
const httpDelete = vi.fn();

vi.mock("@/http", () => ({
  default: {
    get: httpGet,
    post: httpPost,
    put: httpPut,
    delete: httpDelete,
  },
}));

const mockSiteStore = {
  getInfo: vi.fn(),
};

vi.mock("@/stores/site", () => ({
  siteStore: () => mockSiteStore,
}));

const mockAlerts = {
  success: vi.fn(),
};

vi.mock("@/stores/alerts", () => ({
  useAlertsStore: () => mockAlerts,
}));

const inputStub = defineComponent({
  props: ["modelValue", "id", "disabled", "placeholder", "list"],
  emits: ["update:modelValue"],
  template: `
    <label>
      <slot />
      <input
        :id="id"
        :value="modelValue"
        :disabled="disabled"
        :placeholder="placeholder"
        :list="list"
        @input="$emit('update:modelValue', $event.target.value)" />
    </label>
  `,
});

const switchStub = defineComponent({
  props: ["modelValue", "id", "disabled"],
  emits: ["update:modelValue"],
  template: `
    <label>
      <input
        :id="id"
        type="checkbox"
        :checked="modelValue"
        :disabled="disabled"
        @change="$emit('update:modelValue', $event.target.checked)" />
      <slot />
    </label>
  `,
});

const submitButtonStub = defineComponent({
  props: ["disabled", "loading", "title", "block"],
  emits: ["click"],
  setup(props, { emit, slots }) {
    return () =>
      h(
        "button",
        {
          class: "submit-button",
          type: "submit",
          disabled: props.disabled,
          title: props.title,
          onClick: (event: MouseEvent) => emit("click", event),
        },
        slots.default ? slots.default() : undefined,
      );
  },
});

const stubs = {
  TextInput: inputStub,
  PasswordInput: inputStub,
  SwitchInput: switchStub,
  SubmitButton: submitButtonStub,
  SpinnerElement: defineComponent({ template: "<div data-testid='spinner'></div>" }),
  FloatingErrorBanner: defineComponent({
    props: ["visible", "title", "message"],
    template: "<div data-testid='error-banner' v-if='visible'>{{ title }} {{ message }}</div>",
  }),
  "v-btn": defineComponent({
    inheritAttrs: false,
    props: ["disabled", "variant", "color", "prependIcon"],
    emits: ["click"],
    setup(props, { attrs, emit, slots }) {
      return () =>
        h(
          "button",
          {
            ...attrs,
            class: ["v-btn", attrs.class],
            type: "button",
            disabled: props.disabled,
            onClick: (event: MouseEvent) => emit("click", event),
          },
          slots.default ? slots.default() : undefined,
        );
    },
  }),
};

describe("AdminSystem.vue", () => {
  beforeEach(() => {
    httpGet.mockReset();
    httpPost.mockReset();
    httpPut.mockReset();
    httpDelete.mockReset();
    mockAlerts.success.mockReset();
    mockSiteStore.getInfo.mockReset();

    httpGet.mockImplementation((url: string) => {
      if (url === "/api/security/sso") {
        return Promise.resolve({
          data: {
            enabled: false,
            login_path: "/api/user/sso/login",
            login_button_text: "Sign in with SSO",
            provider_login_url: null,
            provider_redirect_param: null,
            auto_create_users: false,
            providers: [],
            role_claims: [],
          },
        });
      }
      if (url === "/api/security/oauth2") {
        return Promise.resolve({
          data: {
            enabled: false,
            login_path: "/api/user/oauth2/login",
            callback_path: "/api/user/oauth2/callback",
            redirect_base_url: null,
            auto_create_users: false,
            google: null,
            microsoft: null,
            casbin: {
              model: "[request_definition]\nr = sub, obj, act",
              policy: "p = sub, obj, act",
            },
            group_role_mappings: [],
            available_roles: [],
          },
        });
      }
      throw new Error(`unexpected GET ${url}`);
    });
  });

  it("adds a generic provider and sends its configuration", async () => {
    httpPut.mockResolvedValue({});
    const module = await import("@/views/admin/AdminSystem.vue");
    const wrapper = mount(module.default, { global: { stubs } });
    await flushPromises();
    await wrapper.get('[data-testid="add-generic-provider"]').trigger("click");
    await wrapper.get('[data-testid="generic-id"] input').setValue("company-sso");
    await wrapper.get('[data-testid="generic-name"] input').setValue("Company sign-in");
    await wrapper.get('[data-testid="generic-issuer"] input').setValue("https://id.example");
    await wrapper.get('[data-testid="generic-client-id"] input').setValue("pkgly");
    await wrapper.get('[data-testid="generic-secret"] input').setValue("secret");
    await wrapper.get("form.oauthForm").trigger("submit");
    await flushPromises();
    expect(httpPut).toHaveBeenCalledWith(
      "/api/security/oauth2",
      expect.objectContaining({
        providers: [
          expect.objectContaining({
            id: "company-sso",
            display_name: "Company sign-in",
            issuer: "https://id.example",
            client_id: "pkgly",
            client_secret: "secret",
          }),
        ],
      }),
    );
  });

  it("loads single sign on settings without fetching webhooks", async () => {
    const module = await import("@/views/admin/AdminSystem.vue");
    const wrapper = mount(module.default, {
      global: {
        stubs,
      },
    });

    await flushPromises();

    expect(wrapper.text()).toContain("Single Sign On");
    expect(wrapper.text()).toContain("OAuth2 Providers");
    expect(wrapper.text()).not.toContain("Package Webhooks");
    expect(httpGet).toHaveBeenCalledWith("/api/security/sso");
    expect(httpGet).toHaveBeenCalledWith("/api/security/oauth2");
    expect(httpGet).not.toHaveBeenCalledWith("/api/system/webhooks");
  });

  it("saves edited Casbin model and policy values", async () => {
    httpPut.mockResolvedValue({});
    const module = await import("@/views/admin/AdminSystem.vue");
    const wrapper = mount(module.default, {
      global: {
        stubs,
      },
    });

    await flushPromises();
    await wrapper.get("#oauth-enabled").setValue(true);
    await wrapper.get("#casbin-model").setValue("model-v2");
    await wrapper.get("#casbin-policy").setValue("policy-v2");
    await wrapper.get("form.oauthForm").trigger("submit");
    await flushPromises();

    expect(httpPut).toHaveBeenCalledWith(
      "/api/security/oauth2",
      expect.objectContaining({
        casbin: { model: "model-v2", policy: "policy-v2" },
      }),
    );
  });
});
