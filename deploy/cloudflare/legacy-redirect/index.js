// Permanent redirect from the old NetRust domain to NetHackED, keeping path and query.
export default {
  fetch(request) {
    const url = new URL(request.url);
    url.hostname = 'nethacked.yemelianov.dev';
    return Response.redirect(url.toString(), 301);
  },
};
