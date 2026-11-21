-- 管理员机制（2026-11-14）：复用 0001 已建的 users.role（'user' / 'admin' / 'root'），
-- 无需新增 is_admin 列；role='root' 在代码层（User::is_admin / is_root）已保证站长拥有全部管理权限。
-- 本迁移只做数据兜底：站长账号 id 10 始终为 root。
UPDATE users SET role = 'root' WHERE id = 10 AND role <> 'root';
