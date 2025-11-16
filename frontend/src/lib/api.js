const API_BASE = '/api';

async function request(path, { method = 'GET', data } = {}) {
  const options = {
    method,
    credentials: 'include',
    headers: {}
  };

  if (data !== undefined) {
    options.body = JSON.stringify(data);
    options.headers['Content-Type'] = 'application/json';
  }

  const response = await fetch(`${API_BASE}${path}`, options);
  const contentType = response.headers.get('content-type') || '';
  const isJson = contentType.includes('application/json');
  const payload = isJson ? await response.json() : await response.text();

  if (!response.ok) {
    const message = payload?.message || payload || '请求失败';
    throw new Error(message);
  }

  return payload;
}

export const api = {
  register(data) {
    return request('/register', { method: 'POST', data });
  },
  login(data) {
    return request('/login', { method: 'POST', data });
  },
  logout() {
    return request('/logout', { method: 'POST' });
  },
  me() {
    return request('/me');
  },
  updateProfile(data) {
    return request('/me', { method: 'PATCH', data });
  },
  changePassword(data) {
    return request('/me/password', { method: 'POST', data });
  },
  myPosts() {
    return request('/me/posts');
  },
  listPosts(category) {
    const query = category ? `?category=${encodeURIComponent(category)}` : '';
    return request(`/posts${query}`);
  },
  createPost(data) {
    return request('/posts', { method: 'POST', data });
  },
  postDetail(id) {
    return request(`/posts/${id}`);
  },
  comment(postId, data) {
    return request(`/posts/${postId}/comments`, { method: 'POST', data });
  },
  adminDelete(postId) {
    return request(`/admin/posts/${postId}`, { method: 'DELETE' });
  },
  publicProfile(uid) {
    return request(`/users/${uid}`);
  }
};

export const CATEGORIES = ['扩列', '吐槽', '表白', '提问', '其它'];
export const ALL_BOARDS = ['最新', ...CATEGORIES];
