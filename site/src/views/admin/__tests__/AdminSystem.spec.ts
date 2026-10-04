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
  "v-select": defineComponent({
    props: ["modelValue", "id", "label", "items", "variant", "density"],
    emits: ["update:modelValue"],
    template: `
      <label>
        {{ label }}
        <select :id="id" :value="modelValue"
          @change="$emit('update:modelValue', $event.target.value)">
          <option v-for="item in items" :key="item.value" :value="item.value">
            {{ item.title }}
          </option>
        </select>
      </label>
    `,
  }),
  SpinnerElement: defineComponent({ template: "<div data-testid='spinner'></div>" }),
  FloatingErrorBanner: defineComponent({
    props: ["visible", "title", "message"],
    template: "<div data-testid='error-banner' v-if='visible'>{{ title }} {{ message }}</div>",
  }),
  "v-btn": defineComponent({
    inheritAttrs: false,
    props: {
      disabled: Boolean,
      variant: String,
      color: String,
      prependIcon: String,
      block: Boolean,
    },
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
    await wrapper.get(".genericProviders select").setValue("client_secret_post");
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
            token_endpoint_auth_method: "client_secret_post",
          }),
        ],
      }),
    );
  });

  it("places the full-width primary add action above custom provider cards", async () => {
    const module = await import("@/views/admin/AdminSystem.vue");
    const wrapper = mount(module.default, { global: { stubs } });
    await flushPromises();
    const section = wrapper.get(".genericProviders");
    const add = section.getComponent('[data-testid="add-generic-provider"]');
    expect(add.props()).toMatchObject({
      color: "primary",
      variant: "outlined",
      prependIcon: "mdi-plus",
      block: true,
    });
    expect(section.classes()).toContain("providerSection");
    expect(add.element.parentElement?.classList.contains("providerSection__header")).toBe(true);
    await add.trigger("click");
    expect(
      add.element.compareDocumentPosition(section.get(".oidcProvider").element) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("places each provider's delete action in the bottom-right action row", async () => {
    const module = await import("@/views/admin/AdminSystem.vue");
    const wrapper = mount(module.default, { global: { stubs } });
    await flushPromises();
    await wrapper.get('[data-testid="add-generic-provider"]').trigger("click");
    await wrapper.get('[data-testid="add-generic-provider"]').trigger("click");
    const cards = wrapper.findAll(".genericProviders .oidcProvider");
    for (const card of cards) {
      const actions = card.get(".providerActions");
      expect(card.element.lastElementChild).toBe(actions.element);
      expect(actions.getComponent(stubs["v-btn"]).props()).toMatchObject({
        color: "error",
        variant: "text",
        prependIcon: "mdi-delete",
      });
    }
    await cards[1].get(".providerActions button").trigger("click");
    expect(wrapper.findAll(".genericProviders .oidcProvider")).toHaveLength(1);
    expect(wrapper.get(".genericProviders .oidcProvider").element).toBe(cards[0].element);
  });

  it("aligns client authentication with the outlined text fields", async () => {
    const module = await import("@/views/admin/AdminSystem.vue");
    const wrapper = mount(module.default, { global: { stubs } });
    await flushPromises();
    await wrapper.get('[data-testid="add-generic-provider"]').trigger("click");
    const authentication = wrapper.getComponent('[data-testid="generic-client-authentication"]');
    expect(authentication.props()).toMatchObject({
      label: "Client authentication",
      variant: "outlined",
      density: "comfortable",
      modelValue: "client_secret_basic",
      items: [
        { title: "HTTP Basic", value: "client_secret_basic" },
        { title: "Request body", value: "client_secret_post" },
      ],
    });
  });

  it("retains a loaded secret while editing, disabling, and removing a custom provider", async () => {
    const originalGet = httpGet.getMockImplementation()!;
    httpGet.mockImplementation(async (url: string) => {
      const response = await originalGet(url);
      if (url === "/api/security/oauth2") {
        response.data.providers = [
          {
            id: "partner-login",
            display_name: "Partner account",
            enabled: true,
            issuer: "https://partner.example/team",
            client_id: "pkgly",
            client_secret_configured: true,
            scopes: ["openid", "profile", "email"],
            token_endpoint_auth_method: "client_secret_post",
            id_token_signing_alg: "RS256",
          },
        ];
        response.data.group_role_mappings = [
          { provider: "partner-login", group: "staff", roles: ["read"] },
        ];
      }
      return response;
    });
    httpPut.mockResolvedValue({});
    const module = await import("@/views/admin/AdminSystem.vue");
    const wrapper = mount(module.default, { global: { stubs } });
    await flushPromises();
    await wrapper.get("#oauth-enabled").setValue(true);
    expect(wrapper.get('[data-testid="generic-secret"] input').attributes("placeholder")).toContain(
      "keep the current secret",
    );
    await wrapper.get('[data-testid="generic-name"] input').setValue("Updated label");
    await wrapper.get('input[id^="generic-enabled-"]').setValue(false);
    await wrapper.get("form.oauthForm").trigger("submit");
    await flushPromises();
    expect(httpPut).toHaveBeenLastCalledWith(
      "/api/security/oauth2",
      expect.objectContaining({
        providers: [
          expect.objectContaining({
            id: "partner-login",
            display_name: "Updated label",
            enabled: false,
            client_secret: null,
          }),
        ],
      }),
    );
    const remove = wrapper.findAll("button").find((button) => button.text() === "Remove provider")!;
    await remove.trigger("click");
    await wrapper.get("form.oauthForm").trigger("submit");
    await flushPromises();
    expect(httpPut).toHaveBeenLastCalledWith(
      "/api/security/oauth2",
      expect.objectContaining({ providers: [], group_role_mappings: [] }),
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
