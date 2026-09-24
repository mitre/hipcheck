<script lang="ts">
	import './layout.css';
  let { children } = $props();
	import favicon from '$lib/assets/favicon.svg';
  import { page } from '$app/stores';


	// Menu Items - Name, Path, and Icons
	const menuItems = [
		{ name: 'Package Sources', path: '/packagesources', icon: "fa fa-archive" },
		{ name: 'Assessments', path: '/assessments' , icon: "fa fa-cube" },
		// Example pages for the demo; comment out these two lines to hide them from the sidebar.
		{ name: 'Example Sources', path: '/sources', icon: "fa fa-folder-open" },
		{ name: 'Example Assessments', path: '/example-assessments', icon: "fa fa-folder-open" },
	];

</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>
<!-- {@render children()} -->

<!-- Style for the Sidebar and Main Content -->
<style>
  .wrapper {
	  flex: 1;
  }
  .header {
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
  <nav class="sidebar">
    <h2>WORKSPACE</h2>
    {#each menuItems as item}
      <a
        href={item.path}
        class:active={$page.url.pathname === item.path}
      >
	  	<i class={item.icon}></i>
    	{item.name}
      </a>
    {/each}
    <!-- <h2>SYSTEM</h2>
    <h2> Data Status</h2> -->
  </nav>

  <div class="wrapper">
  <!-- Header Code -->
  <div class="header">
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