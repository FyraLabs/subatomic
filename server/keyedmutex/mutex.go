package keyedmutex

import "sync"

type KeyedMutex struct {
	sync.Map
}

func (m *KeyedMutex) Lock(key string) {
	mu, _ := m.LoadOrStore(key, &sync.Mutex{})
	mu.(*sync.Mutex).Lock()
}

func (m *KeyedMutex) Unlock(key string) {
	mu, _ := m.Load(key)
	mu.(*sync.Mutex).Unlock()
}
