# Local diagrams

Open this file in simple.pdf. Diagrams render on your device, with no Java installation or diagram server.

## Mermaid flowchart

```mermaid
flowchart LR
    Source[Source file] --> Render[Render local]
    Render --> Preview[Preview]
    Preview --> Export[Export SVG]
```

## PlantUML sequence

```plantuml
@startuml
Alice -> Bob : Review the design
Bob --> Alice : Looks good
@enduml
```

## PlantUML classes

```puml
@startuml
class Document {
  +title: String
  +open()
}
class Diagram {
  +render()
}
Document <|-- Diagram
@enduml
```

## Mermaid state diagram

```mermaid
stateDiagram-v2
    [*] --> Loading
    Loading --> Preview: Success
    Loading --> Error: Invalid source
    Error --> Loading: Reload
    Preview --> [*]: Close
```

Use **Source** below a diagram to inspect or copy its definition. Use **Reload** (F5) after editing the file in another application.
