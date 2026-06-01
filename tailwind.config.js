/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  darkMode: 'class',
  theme: {
    extend: {
      colors: {
        // Compact dark overlay-friendly palette
        bg: '#0f1115',
        'bg-elev': '#16181f',
        'bg-card': '#1c1f28',
        accent: '#3b82f6',
        'accent-hover': '#2563eb',
        success: '#22c55e',
        warning: '#eab308',
        danger: '#ef4444',
        text: '#e5e7eb',
        'text-muted': '#9ca3af',
        border: '#2a2f3a',
      }
    }
  },
  plugins: []
};
