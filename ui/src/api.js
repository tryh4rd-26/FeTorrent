import { useState, useEffect } from 'react';

// REST endpoints
export const fetchTorrents = async () => (await fetch('/api/v1/torrents')).json();
export const fetchStats = async () => (await fetch('/api/v1/stats')).json();
export const pauseTorrent = async (id) => fetch(`/api/v1/torrents/${id}/pause`, { method: 'POST' });
export const resumeTorrent = async (id) => fetch(`/api/v1/torrents/${id}/resume`, { method: 'POST' });
export const removeTorrent = async (id) => fetch(`/api/v1/torrents/${id}`, { method: 'DELETE' });
export const addTorrent = async (formData) => fetch('/api/v1/torrents/add', { method: 'POST', body: formData });
export const fetchSettings = async () => (await fetch('/api/v1/settings')).json();
export const updateSettings = async (settings) => fetch('/api/v1/settings', { 
  method: 'POST', 
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(settings) 
});

// WebSocket Hook
export function useFeTorrentStream() {
  const [torrents, setTorrents] = useState([]);
  const [globalStats, setGlobalStats] = useState({ 
    dl_speed: 0, 
    ul_speed: 0, 
    active_torrents: 0 
  });
  const [connected, setConnected] = useState(false);

  useEffect(() => {
    // Note: Vite proxy routes /ws to localhost:6977
    const wsProt = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
    const wsUrl = `${wsProt}//${window.location.host}/api/v1/ws`;
    
    let ws = null;
    let reconnectTimer = null;

    const connect = () => {
      ws = new WebSocket(wsUrl);

      ws.onopen = () => {
        setConnected(true);
      };

      ws.onmessage = (event) => {
        try {
          const msg = JSON.parse(event.data);
          const data = msg.data;
          
          if (msg.type === 'stats_update') {
             setTorrents(prev => {
                const map = new Map(prev.map(t => [t.id, t]));
                for (const updated of data.torrents) {
                    map.set(updated.id, updated);
                }
                return Array.from(map.values()).sort((a,b) => a.id - b.id);
             });
             setGlobalStats(data.global);
          } else if (msg.type === 'torrent_added') {
             setTorrents(prev => [...prev.filter(t => t.id !== data.torrent.id), data.torrent]);
          } else if (msg.type === 'torrent_removed') {
             setTorrents(prev => prev.filter(t => t.id !== data.id));
          } else if (msg.type === 'torrent_updated') {
             setTorrents(prev => prev.map(t => t.id === data.torrent.id ? data.torrent : t));
          }
        } catch (err) {
          console.error('WS Parse Error', err);
        }
      };

      ws.onclose = () => {
        setConnected(false);
        // auto-reconnect
        reconnectTimer = setTimeout(connect, 3000);
      };
      
      ws.onerror = () => {
        ws.close();
      };
    };

    connect();

    // Initial sync
    fetchTorrents().then(setTorrents).catch(console.error);

    return () => {
      if (reconnectTimer) clearTimeout(reconnectTimer);
      if (ws) ws.close();
    };
  }, []);

  return { torrents, globalStats, connected };
}
