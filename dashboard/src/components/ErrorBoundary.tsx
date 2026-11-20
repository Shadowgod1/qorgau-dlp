import { Component, ReactNode } from 'react'
import { Button, Result } from 'antd'

interface Props { children: ReactNode }
interface State { error: Error | null }

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null }

  static getDerivedStateFromError(error: Error): State {
    return { error }
  }

  render() {
    if (this.state.error) {
      return (
        <Result
          status="error"
          title="Ошибка интерфейса"
          subTitle={this.state.error.message}
          extra={
            <Button type="primary" onClick={() => { this.setState({ error: null }); window.location.reload() }}>
              Перезагрузить
            </Button>
          }
        />
      )
    }
    return this.props.children
  }
}
