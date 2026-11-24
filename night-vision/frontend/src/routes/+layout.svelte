<script lang="ts">
	import './layout.css';
  let { children } = $props();
	import favicon from '$lib/assets/favicon.svg';
  import { page } from '$app/stores';

	// Below 768px the sidebar is hidden behind a menu button in the header.
	let menuOpen = $state(false);


	// Menu Items - Name, Path, and Icons
	const menuItems = [
		{ name: 'Package Sources', path: '/sources', icon: "fa fa-archive" },
		{ name: 'Assessments', path: '/assessments' , icon: "fa fa-cube" },
	];

</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
<!-- {@render children()} -->

<!-- Style for the Sidebar and Main Content -->
<style>
  .wrapper {
	  flex: 1;
	  /* Let the content shrink so wide tables scroll inside their own container. */
	  min-width: 0;
  }
  .header {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    text-align: center;
    background: #1d5b6d;
    color: white;
    font-size: 30px;
    min-height: 8vh;
  }
  .layout {
    display: flex;
    min-height: 100vh;
  }
  .sidebar {
    width: 220px;
    background: #1e293b;
    color: white;
    padding: 1rem;
    flex-shrink: 0;
    /* Keep the navigation in view while the page content scrolls. */
    position: sticky;
    top: 0;
    align-self: flex-start;
    height: 100vh;
    overflow-y: auto;
  }
  .sidebar a {
    display: block;
    padding: 0.5rem 0;
    color: white;
    text-decoration: none;
  }
  .sidebar a.active {
    font-weight: bold;
    color: #38bdf8;
  }
  .content {
    /* flex: 1; */
    padding: 1rem;
    display: flex;
    gap: 20px;
  }
  .menu-button,
  .menu-backdrop {
    display: none;
  }
  @media (max-width: 767px) {
    .menu-button {
      display: block;
      position: absolute;
      left: 1rem;
      background: none;
      border: none;
      color: white;
      font-size: 24px;
      cursor: pointer;
    }
    .sidebar {
      position: fixed;
      left: 0;
      z-index: 50;
      transform: translateX(-100%);
      transition: transform 0.2s ease;
    }
    .sidebar.open {
      transform: none;
    }
    .menu-backdrop {
      display: block;
      position: fixed;
      inset: 0;
      z-index: 40;
      background: rgb(0 0 0 / 0.4);
      border: none;
    }
  }
  .container {
    width: 80%;
    margin: auto;
    padding: 20px;
    background-color: #f4f4f4;
  }

</style>

<!-- Link to the Icons used in Sidebar -->
<link rel="stylesheet" href="https://cdnjs.cloudflare.com/ajax/libs/font-awesome/4.7.0/css/font-awesome.min.css">

<div class="layout">

  <!-- Sidebar Code -->
  <nav id="sidebar-nav" class="sidebar" class:open={menuOpen}>
    <h2>WORKSPACE</h2>
    {#each menuItems as item}
      <a
        href={item.path}
        class:active={$page.url.pathname === item.path || $page.url.pathname.startsWith(`${item.path}/`)}
        aria-current={$page.url.pathname === item.path || $page.url.pathname.startsWith(`${item.path}/`) ? 'page' : undefined}
        onclick={() => (menuOpen = false)}
      >
	  	<i class={item.icon}></i>
    	{item.name}
      </a>
    {/each}
    <!-- <h2>SYSTEM</h2>
    <h2> Data Status</h2> -->
  </nav>
  {#if menuOpen}
    <button class="menu-backdrop" aria-label="Close menu" onclick={() => (menuOpen = false)}></button>
  {/if}

  <div class="wrapper">
  <!-- Header Code -->
  <div class="header">
	<button
		class="menu-button"
		aria-label={menuOpen ? 'Close menu' : 'Open menu'}
		aria-expanded={menuOpen}
		aria-controls="sidebar-nav"
		onclick={() => (menuOpen = !menuOpen)}
	>
		<i class="fa fa-bars"></i>
	</button>
	<h4> NIGHT VISION </h4>
  </div>

  <!-- Page Content Code -->
  <main class="content">
	<div class="wrapper">
		{@render children()}
	</div>

  </main>
  </div>
</div>
