/** 아직 만들지 않은 화면. 무엇이 언제 오는지 분명히 적어 둔다. */

import { Empty } from '@/components/ui'

export function Soon({ title, phase, plan }: { title: string; phase: string; plan: string[] }) {
  return (
    <div className="page">
      <div className="page__head">
        <div>
          <h1 className="page__title">{title}</h1>
          <p className="page__desc">{phase}에서 만듭니다.</p>
        </div>
      </div>
      <div className="card">
        <div className="card__body">
          <Empty title={`${phase} 예정`}>
            <ul style={{ textAlign: 'left', maxWidth: 460, margin: '10px auto 0', lineHeight: 1.9 }}>
              {plan.map((p) => (
                <li key={p}>{p}</li>
              ))}
            </ul>
          </Empty>
        </div>
      </div>
    </div>
  )
}
