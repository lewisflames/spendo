<script lang="ts">
  import * as Card from '$lib/components/ui/card';
</script>

<svelte:head>
  <title>License — Spendo</title>
  <meta name="description" content="MIT License" />
</svelte:head>

<section class="flex flex-col gap-6">
  <h1 class="text-3xl font-bold tracking-tight">MIT License</h1>

  <Card.Root>
    <Card.Content class="flex flex-col gap-4 text-sm text-muted-foreground">
      <p>Copyright &copy; 2026 Lewis Flames</p>

      <p>
        Permission is hereby granted, free of charge, to any person obtaining a copy of this
        software and associated documentation files (the "Software"), to deal in the Software
        without restriction, including without limitation the rights to use, copy, modify, merge,
        publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons
        to whom the Software is furnished to do so, subject to the following conditions:
      </p>

      <p>
        The above copyright notice and this permission notice shall be included in all copies or
        substantial portions of the Software.
      </p>

      <p>
        THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED,
        INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
        PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE
        FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
        OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
        DEALINGS IN THE SOFTWARE.
      </p>
    </Card.Content>
  </Card.Root>
</section>
