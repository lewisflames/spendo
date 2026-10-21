<script lang="ts">
  import { tooltip } from '$lib/attachments/tooltip';
  import * as Card from '$lib/components/ui/card';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';
  import { Switch } from '$lib/components/ui/switch';
  import { Select as SelectPrimitive } from 'bits-ui';
  import IconCardHeader from '$lib/components/shared/icon-card-header.svelte';
  import {
    appSettings,
    saveSettings,
    type AccentId,
    type Density,
    type Theme
  } from '$lib/stores/settings.svelte';
  import { SunMoon, Palette } from '@lucide/svelte';
  import { cn } from '$lib/utils';
  import { modKeyLabel } from '$lib/utils/platform';
  import SettingSelectRow from './setting-select-row.svelte';

  const DEFAULT_CUSTOM_PRIMARY = '#3b82f6';
  const modKey = modKeyLabel();

  const themes: { value: Theme; label: string }[] = [
    { value: 'system', label: 'System' },
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' }
  ];

  const densities: { value: Density; label: string }[] = [
    { value: 'comfortable', label: 'Comfortable' },
    { value: 'compact', label: 'Compact' }
  ];

  const accents: { value: AccentId; label: string; swatch: string }[] = [
    { value: 'zinc', label: 'Zinc', swatch: '#71717a' },
    { value: 'blue', label: 'Blue', swatch: '#2563eb' },
    { value: 'emerald', label: 'Emerald', swatch: '#059669' },
    { value: 'amber', label: 'Amber', swatch: '#d97706' },
    { value: 'rose', label: 'Rose', swatch: '#e11d48' },
    { value: 'violet', label: 'Violet', swatch: '#7c3aed' },
    { value: 'custom', label: 'Custom', swatch: '' }
  ];

  let {
    customDraft = $bindable(appSettings.value.customPrimary ?? DEFAULT_CUSTOM_PRIMARY)
  }: {
    customDraft?: string;
  } = $props();

  let customValid = $derived(/^#[0-9a-fA-F]{6}$/.test(customDraft.trim()));

  function handleAccentSelect(value: AccentId) {
    if (value === 'custom') {
      const seeded = appSettings.value.customPrimary ?? DEFAULT_CUSTOM_PRIMARY;
      customDraft = seeded;
      saveSettings({ accent: 'custom', customPrimary: seeded });
    } else {
      saveSettings({ accent: value });
    }
  }

  function handleCustomPick(value: string) {
    customDraft = value;
    saveSettings({ accent: 'custom', customPrimary: value });
  }

  function handleCustomHexInput(value: string) {
    customDraft = value;
    const hex = value.trim();
    if (/^#[0-9a-fA-F]{6}$/.test(hex)) saveSettings({ accent: 'custom', customPrimary: hex });
  }
</script>

<Card.Root>
  <IconCardHeader title="Appearance">
    {#snippet icon()}
      <SunMoon aria-hidden="true" class="size-4 text-muted-foreground" />
    {/snippet}
  </IconCardHeader>
  <Card.Content class="flex flex-col gap-4">
    <SettingSelectRow
      label="Theme"
      description="Match your system, or go light or dark"
      labelFor="setting-theme"
    >
      <Select.Root
        type="single"
        value={appSettings.value.theme}
        onValueChange={(v) => {
          if (v === 'light' || v === 'dark' || v === 'system') saveSettings({ theme: v });
        }}
        items={themes}
      >
        <Select.Trigger id="setting-theme" class="w-36">
          <SelectPrimitive.Value placeholder="Select theme" />
        </Select.Trigger>
        <Select.Content>
          {#each themes as t (t.value)}
            <Select.Item value={t.value}>{t.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </SettingSelectRow>
    <SettingSelectRow label="Accent" description="A preset, or mix your own">
      <div class="flex items-center gap-1.5" role="radiogroup" aria-label="Accent color">
        {#each accents as a (a.value)}
          <button
            type="button"
            role="radio"
            aria-checked={appSettings.value.accent === a.value}
            {@attach tooltip(a.label)}
            aria-label={a.label}
            onclick={() => handleAccentSelect(a.value)}
            class={cn(
              'flex size-7 items-center justify-center rounded-full border transition-all outline-none focus-visible:ring-3 focus-visible:ring-ring/50',
              appSettings.value.accent === a.value
                ? 'border-primary ring-2 ring-ring/50'
                : 'border-border hover:scale-110 hover:border-muted-foreground'
            )}
          >
            {#if a.value === 'custom'}
              <span
                aria-hidden="true"
                class="flex size-5 items-center justify-center rounded-full"
                style="background: conic-gradient(from 0deg, #f43f5e, #f59e0b, #10b981, #3b82f6, #8b5cf6, #f43f5e)"
              >
                <Palette aria-hidden="true" class="size-3 text-white drop-shadow" />
              </span>
            {:else}
              <span
                aria-hidden="true"
                class="size-5 rounded-full"
                style="background-color: {a.swatch}"
              ></span>
            {/if}
          </button>
        {/each}
      </div>
    </SettingSelectRow>
    {#if appSettings.value.accent === 'custom'}
      <SettingSelectRow label="Custom color" description="Applied to buttons, rings and charts">
        <div class="flex items-center gap-2">
          <input
            type="color"
            aria-label="Pick a custom accent color"
            value={appSettings.value.customPrimary ?? DEFAULT_CUSTOM_PRIMARY}
            oninput={(e) => handleCustomPick(e.currentTarget.value)}
            class="h-8 w-10 cursor-pointer rounded-md border border-input bg-transparent p-1"
          />
          <Input
            id="custom-primary-hex"
            value={customDraft}
            oninput={(e) => handleCustomHexInput(e.currentTarget.value)}
            aria-invalid={!customValid}
            aria-label="Custom accent color hex value"
            placeholder="#3b82f6"
            maxlength={7}
            spellcheck={false}
            class="w-24 font-mono"
          />
        </div>
      </SettingSelectRow>
      {#if !customValid}
        <p aria-live="polite" class="text-xs text-destructive">Enter a hex color like #3b82f6.</p>
      {/if}
    {/if}
    <SettingSelectRow label="Density" description="Squeeze a little more onto each page">
      <Select.Root
        type="single"
        value={appSettings.value.density}
        onValueChange={(v) => {
          if (v === 'comfortable' || v === 'compact') saveSettings({ density: v });
        }}
        items={densities}
      >
        <Select.Trigger class="w-36">
          <SelectPrimitive.Value placeholder="Select density" />
        </Select.Trigger>
        <Select.Content>
          {#each densities as d (d.value)}
            <Select.Item value={d.value}>{d.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
    </SettingSelectRow>
    <SettingSelectRow
      label="Sidebar"
      description={`Start with the navigation collapsed (${modKey}B toggles)`}
    >
      <Switch
        checked={appSettings.value.sidebarCollapsed}
        onCheckedChange={(v) => saveSettings({ sidebarCollapsed: v })}
        aria-label="Start with sidebar collapsed"
      />
    </SettingSelectRow>
  </Card.Content>
</Card.Root>
