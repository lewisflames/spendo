<script lang="ts">
  import {
    Building2,
    Wallet,
    TrendingUp,
    DollarSign,
    Handshake,
    CreditCard,
    Receipt,
    ArrowDownCircle,
    ArrowUpCircle,
    PiggyBank,
    Landmark
  } from '@lucide/svelte';

  let { type, class: className = '' }: { type: string; class?: string } = $props();
</script>

{#if type === 'BANK'}
  <Building2 aria-hidden="true" class={className} />
{:else if type === 'CASH'}
  <Wallet aria-hidden="true" class={className} />
{:else if type === 'STOCK' || type === 'MUTUAL'}
  <TrendingUp aria-hidden="true" class={className} />
{:else if type === 'CURRENCY'}
  <DollarSign aria-hidden="true" class={className} />
{:else if type === 'RECEIVABLE'}
  <Handshake aria-hidden="true" class={className} />
{:else if type === 'CREDIT'}
  <CreditCard aria-hidden="true" class={className} />
{:else if type === 'PAYABLE'}
  <Receipt aria-hidden="true" class={className} />
{:else if type === 'LIABILITY'}
  <Landmark aria-hidden="true" class={className} />
{:else if type === 'INCOME'}
  <ArrowDownCircle aria-hidden="true" class={className} />
{:else if type === 'EXPENSE'}
  <ArrowUpCircle aria-hidden="true" class={className} />
{:else if type === 'EQUITY'}
  <PiggyBank aria-hidden="true" class={className} />
{:else}
  <Building2 aria-hidden="true" class={className} />
{/if}
