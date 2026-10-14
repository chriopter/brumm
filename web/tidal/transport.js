// The SDK retains all media handling; only its approved CDN's byte requests use
// the account-bound native transport. Other network requests retain normal CORS.
function mediaUrl(raw) {
  try {
    const url = new URL(String(raw), location.href);
    if (url.protocol !== 'https:' || url.hostname !== 'sp-ad-fa.audio.tidal.com' || url.port || url.username || url.password || url.hash) return raw;
    const target = btoa(url.href).replaceAll('+','-').replaceAll('/','_').replace(/=+$/,'');
    return location.pathname+'/media/'+target;
  } catch { return raw; }
}
if (typeof XMLHttpRequest !== 'undefined') {
  const open = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function(method,url,...args) { return open.call(this,method,String(method).toUpperCase()==='GET' ? mediaUrl(url) : url,...args); };
}
if (window.fetch) {
  const nativeFetch = window.fetch.bind(window);
  window.fetch = (input,init) => {
    if ((init?.method || input?.method || 'GET').toUpperCase() !== 'GET') return nativeFetch(input,init);
    if (typeof input === 'string' || input instanceof URL) return nativeFetch(mediaUrl(input),init);
    const mapped = mediaUrl(input.url);
    return nativeFetch(mapped === input.url ? input : new Request(mapped,input),init);
  };
}

